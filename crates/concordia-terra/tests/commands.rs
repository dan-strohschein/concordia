//! Command buffers (M2 phase 2.0b, ADR-0069): structural changes from processors, applied at the
//! end of each phase in the phase's plan order. Mass-style deferred commands, from public docs only.

mod common;

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use common::{A, B, Pos, ids, moving_world};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::time::TickRate;
use concordia_terra::{
    CommandBuffer, Phase, QueryError, Schedule, SimLoop, SystemCtx, TerraStats, View, World,
    processor, system,
};

const CHUNK: NonZeroUsize = NonZeroUsize::new(16).unwrap();

fn jobs(threads: usize) -> JobSystem {
    JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap()
}

/// Two independent processors (same wave) that each spawn, plus a later one that counts `A`s.
fn spawning_schedule(seen: Arc<Mutex<Vec<usize>>>) -> Schedule {
    let mut schedule = Schedule::new();
    for (name, base) in [("spawn_low", 0u32), ("spawn_high", 100)] {
        schedule.add_processor(processor(name, Phase::PrePhysics).reads::<Pos>().run(
            move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                let commands = view.commands().unwrap();
                for k in 0..3 {
                    let e = commands.spawn();
                    commands.insert(e, A(base + k));
                }
            },
        ));
    }
    schedule.add_processor(
        processor("count_same_phase", Phase::PrePhysics)
            .reads::<A>()
            .after("spawn_low")
            .after("spawn_high")
            .run({
                let seen = seen.clone();
                move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                    let mut n = 0;
                    view.query::<&A>().unwrap().for_each(|_, _| n += 1);
                    seen.lock().unwrap().push(n);
                }
            }),
    );
    schedule.add_processor(
        processor("count_next_phase", Phase::Physics)
            .reads::<A>()
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                let mut n = 0;
                view.query::<&A>().unwrap().for_each(|_, _| n += 1);
                seen.lock().unwrap().push(n);
            }),
    );
    schedule
}

/// `A` values of all entities, in entity id order.
fn a_values(world: &mut World) -> Vec<(EntityId, u32)> {
    let mut out = Vec::new();
    world
        .query::<&A>()
        .unwrap()
        .for_each(world, |id, a| out.push((id, a.0)));
    out.sort();
    out
}

#[test]
fn deferred_spawns_apply_at_phase_end_in_deterministic_order() {
    let run = |threads: usize| {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut world = moving_world(4);
        let mut sim =
            SimLoop::new(TickRate::HZ_60, spawning_schedule(seen.clone()), &mut world).unwrap();
        sim.step(&mut world, &jobs(threads), 1);
        let seen = seen.lock().unwrap().clone();
        (seen, a_values(&mut world), world.state_hash().unwrap())
    };
    let (seen, values, hash) = run(1);
    assert_eq!(
        seen,
        [0, 6],
        "same phase sees nothing; the next phase sees all six"
    );
    // Spawn ids follow plan order (registration order here), whichever thread finished first.
    let by_spawn_order: Vec<u32> = {
        let mut v = values.clone();
        v.sort_by_key(|(id, _)| *id);
        v.into_iter().map(|(_, a)| a).collect()
    };
    assert_eq!(by_spawn_order, [0, 1, 2, 100, 101, 102]);
    for threads in [2, 8] {
        let (s, v, h) = run(threads);
        assert_eq!(
            (s, v, h),
            (seen.clone(), values.clone(), hash),
            "{threads} threads"
        );
    }
}

#[test]
fn commands_on_dead_entities_are_skipped_and_counted() {
    let mut world = moving_world(2);
    let [alive, dead] = world.entity_ids_sorted().try_into().unwrap();
    world.despawn(dead).unwrap();
    let mut commands = CommandBuffer::new();
    commands.insert(dead, A(1));
    commands.insert(alive, A(2));
    commands.remove::<Pos>(dead);
    commands.despawn(dead);
    let report = commands.apply(&mut world, &mut DeterministicIdGen::new(3));
    assert_eq!(report.skipped, 3);
    assert_eq!(world.get::<A>(alive), Some(&A(2)));
}

#[test]
fn skipped_commands_are_counted_in_terra_stats() {
    let mut schedule = Schedule::new();
    schedule.add_processor(processor("bad", Phase::PostPhysics).run(
        |view: &mut View<'_>, _: &SystemCtx<'_>| {
            let gone = ids(99)[98];
            view.commands().unwrap().insert(gone, A(0));
        },
    ));
    let mut world = World::new();
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 2);
    assert_eq!(world.resource::<TerraStats>().unwrap().skipped_commands, 2);
}

#[test]
fn pending_spawns_take_components_and_despawns_apply_in_order() {
    let mut world = World::new();
    let mut commands = CommandBuffer::new();
    let e = commands.spawn();
    commands.insert(e, A(1));
    commands.insert(e, B(2.0));
    commands.remove::<B>(e);
    let f = commands.spawn();
    commands.insert(f, A(9));
    commands.despawn(f);
    let report = commands.apply(&mut world, &mut DeterministicIdGen::new(7));
    assert_eq!(report.spawned.len(), 2);
    assert_eq!(world.len(), 1);
    let id = report.spawned[0];
    assert_eq!(world.get::<A>(id), Some(&A(1)));
    assert_eq!(world.get::<B>(id), None);
    assert_eq!(report.skipped, 0);
}

#[test]
fn appended_buffers_keep_their_pending_spawns_apart() {
    let mut first = CommandBuffer::new();
    let a = first.spawn();
    first.insert(a, A(1));
    let mut second = CommandBuffer::new();
    let b = second.spawn();
    second.insert(b, A(2));
    first.append(second);
    let mut world = World::new();
    let report = first.apply(&mut world, &mut DeterministicIdGen::new(1));
    let values: Vec<u32> = report
        .spawned
        .iter()
        .map(|id| world.get::<A>(*id).unwrap().0)
        .collect();
    assert_eq!(values, [1, 2]);
}

#[test]
fn par_for_each_deferred_orders_commands_by_chunk_not_thread() {
    let run = |threads: usize| {
        let mut schedule = Schedule::new();
        schedule.add_processor(processor("tag_odd", Phase::PostPhysics).reads::<Pos>().run(
            |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                view.query::<&Pos>().unwrap().par_for_each_deferred(
                    ctx.jobs,
                    CHUNK,
                    |commands, id, p| {
                        if p.x as u32 % 2 == 1 {
                            let child = commands.spawn();
                            commands.insert(child, A(p.x as u32));
                            commands.insert(id, B(p.x));
                        }
                    },
                );
            },
        ));
        let mut world = moving_world(200);
        let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
        sim.step(&mut world, &jobs(threads), 1);
        (a_values(&mut world), world.state_hash().unwrap())
    };
    let (values, hash) = run(1);
    assert_eq!(values.len(), 100);
    let mut by_id = values.clone();
    by_id.sort_by_key(|(id, _)| *id);
    let order: Vec<u32> = by_id.into_iter().map(|(_, a)| a).collect();
    assert_eq!(
        order,
        (0..100).map(|k| 2 * k + 1).collect::<Vec<_>>(),
        "spawned in row order"
    );
    assert_eq!(run(8), (values, hash));
}

#[test]
fn presentation_processors_have_no_command_buffer() {
    let result = Arc::new(Mutex::new(None));
    let mut schedule = Schedule::new();
    let out = result.clone();
    schedule.add_processor(processor("draw", Phase::Presentation).run(
        move |view: &mut View<'_>, _: &SystemCtx<'_>| {
            *out.lock().unwrap() = Some(view.commands().map(|_| ()).unwrap_err());
        },
    ));
    let mut world = World::new();
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    assert!(matches!(
        result.lock().unwrap().take(),
        Some(QueryError::CommandsInPresentation { .. })
    ));
}

#[test]
fn exclusive_systems_see_flushed_commands_of_earlier_phases() {
    let seen = Arc::new(Mutex::new(0usize));
    let mut schedule = Schedule::new();
    schedule.add_processor(processor("spawn", Phase::PrePhysics).run(
        |view: &mut View<'_>, _: &SystemCtx<'_>| {
            let commands = view.commands().unwrap();
            let e = commands.spawn();
            commands.insert(e, A(5));
        },
    ));
    let out = seen.clone();
    schedule.add(system(
        "count",
        move |world: &mut World, _: &SystemCtx<'_>| {
            *out.lock().unwrap() = world.len();
        },
    ));
    let mut world = World::new();
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    assert_eq!(*seen.lock().unwrap(), 1);
}
