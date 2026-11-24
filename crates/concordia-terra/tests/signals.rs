//! Signals and observers (M2 phase 2.0b, ADR-0069). Mass-style signals (raised on entities,
//! delivered next tick) and add/remove observers, from public documentation only.

mod common;

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use common::{A, B, Pos, ids, moving_world};
use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;
use concordia_core::time::TickRate;
use concordia_reflect::Reflect;
use concordia_terra::{
    Observe, Phase, QueryError, Schedule, SimLoop, SystemCtx, View, World, WorldHash, observer,
    processor, system,
};
use serde::{Deserialize, Serialize};

/// A test signal with a payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Hit(pub u32);

fn jobs(threads: usize) -> JobSystem {
    JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap()
}

type Log = Arc<Mutex<Vec<(u64, EntityId, u32)>>>;

/// Raises `Hit` on entities in reverse id order at tick 0, and logs every delivery.
fn signal_schedule(log: Log) -> Schedule {
    let mut schedule = Schedule::new();
    schedule.add_processor(processor("raise", Phase::PostPhysics).reads::<Pos>().run(
        |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
            if ctx.tick.0 != 0 {
                return;
            }
            let mut targets = Vec::new();
            view.query::<&Pos>()
                .unwrap()
                .for_each(|id, _| targets.push(id));
            targets.sort();
            let commands = view.commands().unwrap();
            for (k, id) in targets.into_iter().rev().enumerate() {
                commands.signal(id, Hit(k as u32));
                commands.signal(id, Hit(100 + k as u32));
            }
        },
    ));
    schedule.add_processor(
        processor("receive", Phase::PrePhysics)
            .receives::<Hit>()
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                for (id, hit) in view.signals::<Hit>().unwrap() {
                    log.lock().unwrap().push((ctx.tick.0, *id, hit.0));
                }
            }),
    );
    schedule
}

#[test]
fn signal_raised_this_tick_is_delivered_next_tick_in_entity_order() {
    let run = |threads: usize| {
        let log: Log = Arc::default();
        let mut world = moving_world(5);
        let mut sim =
            SimLoop::new(TickRate::HZ_60, signal_schedule(log.clone()), &mut world).unwrap();
        sim.step(&mut world, &jobs(threads), 3);
        let out = log.lock().unwrap().clone();
        (out, world.state_hash().unwrap())
    };
    let (log, hash) = run(1);
    let ids = ids(5);
    // Delivered once, at tick 1 (raised at tick 0), sorted by entity, then raise order.
    let expected: Vec<(u64, EntityId, u32)> = ids
        .iter()
        .enumerate()
        .flat_map(|(i, &id)| {
            let k = (4 - i) as u32;
            [(1, id, k), (1, id, 100 + k)]
        })
        .collect();
    assert_eq!(log, expected);
    assert_eq!(run(8), (log, hash));
}

#[test]
fn pending_signals_are_part_of_the_world_hash() {
    let mut quiet = moving_world(2);
    let mut loud = quiet.clone();
    let target = quiet.entity_ids_sorted()[0];
    assert_eq!(quiet.state_hash().unwrap(), loud.state_hash().unwrap());
    loud.signal(target, Hit(1));
    assert_ne!(quiet.state_hash().unwrap(), loud.state_hash().unwrap());
    quiet.signal(target, Hit(2));
    assert_ne!(
        quiet.state_hash().unwrap(),
        loud.state_hash().unwrap(),
        "payloads are hashed"
    );
}

#[test]
fn signals_to_despawned_entities_are_dropped_and_exclusive_systems_read_them() {
    let got = Arc::new(Mutex::new(Vec::new()));
    let mut world = moving_world(3);
    let [a, b, c] = world.entity_ids_sorted().try_into().unwrap();
    let mut schedule = Schedule::new();
    let out = got.clone();
    schedule.add(system(
        "read",
        move |world: &mut World, _: &SystemCtx<'_>| {
            out.lock()
                .unwrap()
                .extend(world.signals::<Hit>().iter().map(|(id, h)| (*id, h.0)));
        },
    ));
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    world.signal(c, Hit(3));
    world.signal(a, Hit(1));
    world.signal(b, Hit(2));
    world.despawn(b).unwrap();
    sim.step(&mut world, &jobs(1), 2);
    assert_eq!(
        *got.lock().unwrap(),
        [(a, 1), (c, 3)],
        "delivered once, b dropped"
    );
}

#[test]
fn undeclared_signals_are_an_error() {
    let err = Arc::new(Mutex::new(None));
    let out = err.clone();
    let mut schedule = Schedule::new();
    schedule.add_processor(processor("nosy", Phase::PostPhysics).run(
        move |view: &mut View<'_>, _: &SystemCtx<'_>| {
            *out.lock().unwrap() = view.signals::<Hit>().err();
        },
    ));
    let mut world = World::new();
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    assert!(matches!(
        err.lock().unwrap().take(),
        Some(QueryError::Undeclared { component: "Hit" })
    ));
}

#[test]
fn processors_reach_signaled_entities_components() {
    let mut world = moving_world(4);
    let [_, b, _, d] = world.entity_ids_sorted().try_into().unwrap();
    let mut schedule = Schedule::new();
    schedule.add_processor(
        processor("damage", Phase::PostPhysics)
            .receives::<Hit>()
            .writes::<Pos>()
            .run(|view: &mut View<'_>, _: &SystemCtx<'_>| {
                let hits: Vec<(EntityId, u32)> = view
                    .signals::<Hit>()
                    .unwrap()
                    .iter()
                    .map(|(id, h)| (*id, h.0))
                    .collect();
                let mut targets: Vec<EntityId> = hits.iter().map(|(id, _)| *id).collect();
                targets.dedup();
                view.query::<&mut Pos>()
                    .unwrap()
                    .for_each_of(&targets, |id, p| {
                        p.y += hits
                            .iter()
                            .filter(|(t, _)| *t == id)
                            .map(|(_, h)| *h as f32)
                            .sum::<f32>();
                    });
            }),
    );
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    world.signal(d, Hit(4));
    world.signal(b, Hit(2));
    world.signal(b, Hit(1));
    sim.step(&mut world, &jobs(1), 1);
    let ys: Vec<f32> = world
        .entity_ids_sorted()
        .iter()
        .map(|id| world.get::<Pos>(*id).unwrap().y)
        .collect();
    assert_eq!(ys, [0.0, 3.0, 0.0, 4.0]);
}

type Fired = Arc<Mutex<Vec<(&'static str, u64, EntityId)>>>;

fn observing_schedule(fired: Fired) -> Schedule {
    let mut schedule = Schedule::new();
    for (name, on) in [("on_add_a", Observe::Add), ("on_remove_a", Observe::Remove)] {
        let fired = fired.clone();
        schedule.add_processor(observer::<A>(name, on).run(
            move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                for id in view.observed() {
                    fired.lock().unwrap().push((name, ctx.tick.0, *id));
                }
            },
        ));
    }
    schedule.add_processor(processor("churn", Phase::PrePhysics).reads::<Pos>().run(
        |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
            let mut all = Vec::new();
            view.query::<&Pos>().unwrap().for_each(|id, _| all.push(id));
            all.sort();
            let commands = view.commands().unwrap();
            match ctx.tick.0 {
                // Add A twice to the first entity, and add then remove it on the second.
                0 => {
                    commands.insert(all[0], A(1));
                    commands.insert(all[0], A(2));
                    commands.insert(all[1], A(1));
                    commands.remove::<A>(all[1]);
                }
                // Replace the value: no event.
                1 => commands.insert(all[0], A(3)),
                // Despawn the entity that has A.
                2 => commands.despawn(all[0]),
                _ => {}
            }
        },
    ));
    schedule
}

#[test]
fn observers_fire_on_add_and_remove_once() {
    let run = |threads: usize| {
        let fired: Fired = Arc::default();
        let mut world = moving_world(3);
        let first = world.entity_ids_sorted()[0];
        let mut sim = SimLoop::new(
            TickRate::HZ_60,
            observing_schedule(fired.clone()),
            &mut world,
        )
        .unwrap();
        sim.step(&mut world, &jobs(threads), 4);
        let out = fired.lock().unwrap().clone();
        (out, first, world.state_hash().unwrap())
    };
    let (fired, first, hash) = run(1);
    assert_eq!(fired, [("on_add_a", 0, first), ("on_remove_a", 2, first)]);
    let (f8, _, h8) = run(8);
    assert_eq!((f8, h8), (fired, hash));
}

#[test]
fn observers_see_direct_changes_and_their_commands_cascade() {
    let fired: Fired = Arc::default();
    let mut schedule = Schedule::new();
    let log = fired.clone();
    schedule.add_processor(observer::<A>("a_adds_b", Observe::Add).run(
        move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
            let observed = view.observed().to_vec();
            for id in observed {
                log.lock().unwrap().push(("a_adds_b", ctx.tick.0, id));
                view.commands().unwrap().insert(id, B(1.0));
            }
        },
    ));
    let log = fired.clone();
    schedule.add_processor(observer::<B>("b_seen", Observe::Add).run(
        move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
            for id in view.observed() {
                log.lock().unwrap().push(("b_seen", ctx.tick.0, *id));
            }
        },
    ));
    schedule.add_in(
        Phase::Physics,
        system("direct", |world: &mut World, ctx: &SystemCtx<'_>| {
            if ctx.tick.0 == 0 {
                let id = world.entity_ids_sorted()[0];
                world.insert(id, A(7)).unwrap();
            }
        }),
    );
    let mut world = moving_world(1);
    let id = world.entity_ids_sorted()[0];
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    assert_eq!(
        *fired.lock().unwrap(),
        [("a_adds_b", 0, id), ("b_seen", 0, id)]
    );
    assert_eq!(world.get::<B>(id), Some(&B(1.0)));
}

#[test]
fn hash_identical_for_1_and_8_threads_with_commands_signals_and_observers() {
    let run = |threads: usize| -> (Vec<WorldHash>, usize, usize, usize) {
        let fired: Fired = Arc::default();
        let log: Log = Arc::default();
        let mut schedule = observing_schedule(fired.clone());
        schedule.append(signal_schedule(log.clone()));
        let mut world = moving_world(64);
        let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
        let hashes = (0..5)
            .map(|_| {
                sim.step(&mut world, &jobs(threads), 1);
                world.state_hash().unwrap()
            })
            .collect();
        let (f, l) = (fired.lock().unwrap().len(), log.lock().unwrap().len());
        (hashes, f, l, world.len())
    };
    let one = run(1);
    // Not vacuous: observers fired, signals arrived, and a command despawned an entity.
    assert_eq!((one.1, one.2, one.3), (2, 128, 63));
    assert_eq!(run(8), one);
}

/// Signals raised and not yet delivered are counted by type name (M3 3.2b: the assertions'
/// `event("Hit")` means "raised on this tick"): two `Hit`s on two entities count 2, an unknown
/// type 0, and after delivery none are pending.
#[test]
fn pending_signals_are_counted_by_type_name() {
    let mut world = moving_world(2);
    let ids = world.entity_ids_sorted();
    world.signal(ids[0], Hit(1));
    world.signal(ids[1], Hit(2));
    assert_eq!(world.pending_signal_count("Hit"), 2);
    assert_eq!(world.pending_signal_count("Miss"), 0);
    let mut sim = SimLoop::new(TickRate::HZ_60, Schedule::new(), &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    assert_eq!(
        world.pending_signal_count("Hit"),
        0,
        "delivered on the tick"
    );
}
