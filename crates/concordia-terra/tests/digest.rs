//! State digests (per-part hashes for divergence reports) and end-of-tick hooks (M2 phase 2.8).

mod common;

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use common::{Pos, Vel, moving_world};
use concordia_core::jobs::JobSystem;
use concordia_core::rng::SimRng;
use concordia_core::time::TickRate;
use concordia_terra::{Schedule, SimClock, SimLoop, SystemCtx, World, WorldHash, system, tick_end};

fn jobs() -> JobSystem {
    JobSystem::new(NonZeroUsize::MIN).unwrap()
}

fn differing(a: &World, b: &World) -> Vec<String> {
    let (a, b) = (a.state_digest().unwrap(), b.state_digest().unwrap());
    let mut names: Vec<String> = a
        .parts
        .iter()
        .filter(|(name, hash)| b.parts.get(*name) != Some(hash))
        .map(|(name, _)| name.clone())
        .collect();
    names.extend(
        b.parts
            .keys()
            .filter(|n| !a.parts.contains_key(*n))
            .cloned(),
    );
    names
}

#[test]
fn state_digest_hash_is_the_state_hash() {
    let mut world = moving_world(50);
    world.insert_resource(SimRng::new(3));
    let digest = world.state_digest().unwrap();
    assert_eq!(digest.world, world.state_hash().unwrap());
    assert!(
        digest.parts.contains_key("Pos"),
        "{:?}",
        digest.parts.keys()
    );
    assert!(digest.parts.contains_key("Vel"));
}

#[test]
fn digest_parts_change_only_for_the_component_that_changed() {
    let before = moving_world(50);
    let mut after = before.clone();
    let id = after.entity_ids_sorted()[0];
    after.get_mut::<Vel>(id).unwrap().x += 1.0;
    assert_eq!(differing(&before, &after), ["Vel"]);
    after.get_mut::<Pos>(id).unwrap().y -= 1.0;
    assert_eq!(differing(&before, &after), ["Pos", "Vel"]);
}

#[test]
fn digest_parts_cover_entities_clock_and_rng() {
    let mut before = moving_world(10);
    before.insert_resource(SimRng::new(9));
    let mut spawned = before.clone();
    spawned.spawn_with_id(common::ids(11)[10]).unwrap();
    assert_eq!(differing(&before, &spawned), ["@entities"]);

    let mut rolled = before.clone();
    rolled.resource_mut::<SimRng>().unwrap().next_f32();
    assert_eq!(differing(&before, &rolled), ["@rng"]);

    let mut clocked = before.clone();
    clocked.insert_resource(SimClock::new(TickRate::HZ_60));
    assert_eq!(differing(&before, &clocked), ["@clock"]);
}

#[test]
fn tick_end_hooks_run_after_every_tick_with_the_advanced_clock() {
    let seen: Arc<Mutex<Vec<(u64, WorldHash)>>> = Arc::default();
    let mut schedule = Schedule::new();
    schedule.add(system("drift", |world: &mut World, _: &SystemCtx| {
        let mut query = world.query::<&mut Pos>().unwrap();
        query.for_each(world, |_, p| p.x += 1.0);
    }));
    let log = Arc::clone(&seen);
    schedule.add_tick_end(tick_end("record", move |world: &World| {
        let tick = world.resource::<SimClock>().unwrap().tick.0;
        log.lock()
            .unwrap()
            .push((tick, world.state_hash().unwrap()));
    }));
    let mut world = moving_world(20);
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(), 3);
    let seen = seen.lock().unwrap();
    let ticks: Vec<u64> = seen.iter().map(|(t, _)| *t).collect();
    assert_eq!(ticks, [1, 2, 3], "hooks see the clock after it advances");
    assert_eq!(
        seen[2].1,
        world.state_hash().unwrap(),
        "the hash sim.step reports"
    );
    assert_ne!(seen[0].1, seen[1].1);
}

#[test]
fn appended_schedules_keep_their_tick_end_hooks() {
    let count = Arc::new(Mutex::new(0u32));
    let mut module = Schedule::new();
    let c = Arc::clone(&count);
    module.add_tick_end(tick_end("count", move |_: &World| *c.lock().unwrap() += 1));
    let mut schedule = Schedule::new();
    schedule.append(module);
    assert_eq!(schedule.tick_end_names(), ["count"]);
    let mut world = World::new();
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(), 4);
    assert_eq!(*count.lock().unwrap(), 4);
}

/// An observed tick (M3 3.1c, for naming the system that diverged) reports each wave by its
/// systems' names and each simulation phase's command flush, in run order, and ends in the same
/// world as an ordinary tick: Terra's built-in "terra.timers" and "first" (PrePhysics), its
/// flush, "second" (PostPhysics), its flush;
/// the hash after "first" already differs from the start, and the last equals a plain step's.
#[test]
fn observed_ticks_report_each_system_and_phase_flush() {
    use concordia_terra::Phase;
    let schedule = || {
        let mut s = Schedule::new();
        s.add_in(
            Phase::PrePhysics,
            system("first", |world: &mut World, _: &SystemCtx| {
                let mut query = world.query::<&mut Pos>().unwrap();
                query.for_each(world, |_, p| p.x += 1.0);
            }),
        );
        s.add_in(
            Phase::PostPhysics,
            system("second", |world: &mut World, _: &SystemCtx| {
                let mut query = world.query::<&mut Pos>().unwrap();
                query.for_each(world, |_, p| p.y += 1.0);
            }),
        );
        s
    };
    let mut plain = moving_world(5);
    let mut observed = plain.clone();
    let start = observed.state_hash().unwrap();
    let mut a = SimLoop::new(TickRate::HZ_60, schedule(), &mut plain).unwrap();
    a.step(&mut plain, &jobs(), 1);
    let mut b = SimLoop::new(TickRate::HZ_60, schedule(), &mut observed).unwrap();
    let mut seen: Vec<(String, WorldHash)> = Vec::new();
    b.step_observed(&mut observed, &jobs(), &mut |label: &str, w: &World| {
        seen.push((label.to_owned(), w.state_hash().unwrap()));
    });
    let labels: Vec<&str> = seen.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(
        labels,
        [
            "terra.timers",
            "first",
            "PrePhysics commands",
            "second",
            "PostPhysics commands"
        ]
    );
    assert_ne!(seen[1].1, start, "after first");
    assert_eq!(observed.state_hash().unwrap(), plain.state_hash().unwrap());
}
