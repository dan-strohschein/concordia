//! Timers (M2 phase 2.0): fire on the tick they expire, in scaled simulation time.

mod common;

use std::num::NonZeroUsize;

use concordia_core::id::DeterministicIdGen;
use concordia_core::jobs::JobSystem;
use concordia_core::time::TickRate;
use concordia_terra::{Schedule, SimClock, SimLoop, TimeScale, Timer, World};

fn jobs() -> JobSystem {
    JobSystem::new(NonZeroUsize::new(2).unwrap()).unwrap()
}

/// A world with one timer, and a loop that runs only the built-in processors.
fn one_timer(timer: Timer) -> (World, SimLoop, concordia_core::id::EntityId) {
    let mut world = World::new();
    let id = world.spawn(&mut DeterministicIdGen::new(3)).unwrap();
    world.insert(id, timer).unwrap();
    let sim = SimLoop::new(TickRate::HZ_60, Schedule::new(), &mut world).unwrap();
    (world, sim, id)
}

#[test]
fn a_one_shot_timer_fires_once_on_the_tick_it_expires() {
    let (mut world, mut sim, id) = one_timer(Timer::once(0.5).unwrap());
    let jobs = jobs();
    let mut fired_at = Vec::new();
    for _ in 0..90 {
        sim.step(&mut world, &jobs, 1);
        if world.get::<Timer>(id).unwrap().fired() > 0 {
            fired_at.push(world.resource::<SimClock>().unwrap().tick.0);
        }
    }
    assert_eq!(fired_at, [30], "0.5 s at 60 Hz is the 30th tick, once");
    assert!(world.get::<Timer>(id).unwrap().finished());
}

#[test]
fn a_repeating_timer_fires_every_period_and_several_times_in_a_large_step() {
    let (mut world, mut sim, id) = one_timer(Timer::repeating(0.05).unwrap());
    let jobs = jobs();
    sim.step(&mut world, &jobs, 3);
    assert_eq!(
        world.get::<Timer>(id).unwrap().fired(),
        1,
        "3 ticks = 0.05 s"
    );
    world.resource_mut::<SimClock>().unwrap().time_scale = TimeScale::new(10.0).unwrap();
    sim.step(&mut world, &jobs, 1);
    // One tick at 10x is 1/6 s: three full periods of 0.05 s.
    assert_eq!(world.get::<Timer>(id).unwrap().fired(), 3);
    assert!(!world.get::<Timer>(id).unwrap().finished());
}

#[test]
fn a_frozen_clock_never_fires_and_timers_are_deterministic() {
    let (mut world, mut sim, id) = one_timer(Timer::once(0.1).unwrap());
    world.resource_mut::<SimClock>().unwrap().time_scale = TimeScale::new(0.0).unwrap();
    sim.step(&mut world, &jobs(), 600);
    assert_eq!(
        world.get::<Timer>(id).unwrap().fired(),
        0,
        "hit-stop freezes timers"
    );
    assert!(!world.get::<Timer>(id).unwrap().finished());

    let hash = |threads: usize| {
        let (mut world, mut sim, _) = one_timer(Timer::repeating(0.07).unwrap());
        let jobs = JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap();
        sim.step(&mut world, &jobs, 200);
        world.state_hash().unwrap()
    };
    assert_eq!(hash(1), hash(8));
}

#[test]
fn bad_durations_are_errors() {
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(Timer::once(bad).is_err(), "once({bad})");
        assert!(Timer::repeating(bad).is_err(), "repeating({bad})");
    }
}
