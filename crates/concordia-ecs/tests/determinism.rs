//! Deterministic fixed-step simulation and world hashing (M0 phase 0.3).

mod common;

use std::num::NonZeroUsize;

use common::{A, B, Pos, Vel, ids, moving_world};
use concordia_core::jobs::JobSystem;
use concordia_core::rng::SimRng;
use concordia_core::time::{Tick, TickRate};
use concordia_ecs::{Schedule, SimClock, SimLoop, SystemCtx, World, system};

const CHUNK: NonZeroUsize = NonZeroUsize::new(1024).unwrap();

fn schedule() -> Schedule {
    let mut schedule = Schedule::new();
    schedule.add(system("integrate", |world: &mut World, ctx: &SystemCtx| {
        let mut query = world.query::<(&mut Pos, &Vel)>().expect("disjoint");
        let dt = ctx.dt;
        query.par_for_each(world, ctx.jobs, CHUNK, |_, (p, v)| {
            p.x += v.x * dt;
            p.y += v.y * dt;
            p.z += v.z * dt;
        });
    }));
    schedule.add(system("jitter", |world: &mut World, _: &SystemCtx| {
        world.resource_scope(|world: &mut World, rng: &mut SimRng| {
            let mut query = world.query::<&mut Vel>().expect("single");
            query.for_each(world, |_, v| v.y += rng.next_f32() - 0.5);
        });
    }));
    schedule
}

fn simulate(threads: usize, ticks: &[u32]) -> Vec<concordia_ecs::WorldHash> {
    let mut world = moving_world(5_000);
    world.insert_resource(SimRng::new(77));
    let jobs = JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap();
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule());
    let mut done = 0;
    ticks
        .iter()
        .map(|&target| {
            sim.step(&mut world, &jobs, target - done);
            done = target;
            world.state_hash().unwrap()
        })
        .collect()
}

#[test]
fn same_inputs_same_hash_after_1000_ticks() {
    assert_eq!(simulate(4, &[1000]), simulate(4, &[1000]));
}

#[test]
fn hash_identical_across_thread_counts() {
    let checkpoints = [1, 10, 100, 1000];
    let expected = simulate(1, &checkpoints);
    for threads in [2, 4, 8] {
        assert_eq!(
            simulate(threads, &checkpoints),
            expected,
            "threads = {threads}"
        );
    }
}

#[test]
fn hash_independent_of_spawn_and_archetype_history() {
    let ids = ids(3);
    let mut direct = World::new();
    for &id in &ids {
        direct
            .spawn_with_id(id)
            .unwrap()
            .insert(A(1))
            .unwrap()
            .insert(B(2.0))
            .unwrap();
    }
    let mut detour = World::new();
    for &id in ids.iter().rev() {
        detour.spawn_with_id(id).unwrap().insert(B(9.0)).unwrap();
        detour.insert(id, Pos::default()).unwrap();
        detour.insert(id, A(1)).unwrap();
        detour.remove::<Pos>(id).unwrap();
        detour.insert(id, B(2.0)).unwrap();
    }
    assert_eq!(direct.state_hash().unwrap(), detour.state_hash().unwrap());
}

#[test]
fn hash_changes_when_any_component_bit_changes() {
    let mut world = moving_world(10);
    let before = world.state_hash().unwrap();
    let pos = world.get_mut::<Pos>(ids(1)[0]).unwrap();
    pos.y = f32::from_bits(pos.y.to_bits() + 1);
    assert_ne!(world.state_hash().unwrap(), before);
}

#[test]
fn hash_covers_clock_and_rng() {
    let mut world = World::new();
    let base = world.state_hash().unwrap();
    world.insert_resource(SimRng::new(1));
    let with_rng = world.state_hash().unwrap();
    assert_ne!(base, with_rng);
    world.resource_mut::<SimRng>().unwrap().next_u32();
    assert_ne!(
        world.state_hash().unwrap(),
        with_rng,
        "rng position is hashed"
    );
}

#[test]
fn sim_loop_steps_exactly_n_ticks() {
    let mut world = World::new();
    let jobs = JobSystem::new(NonZeroUsize::new(1).unwrap()).unwrap();
    let mut sim = SimLoop::new(TickRate::HZ_60, Schedule::new());
    sim.step(&mut world, &jobs, 7);
    assert_eq!(world.resource::<SimClock>().unwrap().tick, Tick(7));
    let steps = sim.advance(&mut world, &jobs, std::time::Duration::from_millis(50));
    assert_eq!(steps.steps, 3);
    assert_eq!(world.resource::<SimClock>().unwrap().tick, Tick(10));
}

#[test]
fn systems_run_in_declared_order() {
    let mut order = Schedule::new();
    order.add(system("first", |w: &mut World, _: &SystemCtx| {
        w.insert_resource(vec!["first"]);
    }));
    order.add(system("second", |w: &mut World, _: &SystemCtx| {
        w.resource_mut::<Vec<&str>>().unwrap().push("second");
    }));
    assert_eq!(order.names(), vec!["first", "second"]);
    let mut world = World::new();
    let jobs = JobSystem::new(NonZeroUsize::new(1).unwrap()).unwrap();
    SimLoop::new(TickRate::HZ_60, order).step(&mut world, &jobs, 1);
    assert_eq!(
        world.resource::<Vec<&str>>().unwrap(),
        &vec!["first", "second"]
    );
}

#[test]
fn world_hash_hex_and_serde_roundtrip() {
    let hash = moving_world(3).state_hash().unwrap();
    let hex = hash.to_string();
    assert_eq!(hex.len(), 64);
    assert_eq!(hex.parse::<concordia_ecs::WorldHash>().unwrap(), hash);
    let json = serde_json::to_string(&hash).unwrap();
    assert_eq!(json, format!("\"{hex}\""));
    assert_eq!(
        serde_json::from_str::<concordia_ecs::WorldHash>(&json).unwrap(),
        hash
    );
    assert!("zz".parse::<concordia_ecs::WorldHash>().is_err());
}
