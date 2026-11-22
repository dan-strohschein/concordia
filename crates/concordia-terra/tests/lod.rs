//! Simulation LOD (M2 phase 2.4b.1, ADR-0076). Every expected tick, level and elapsed time below
//! is hand-computed. The stagger phases of the test ids (`common::ids`) were computed separately
//! (splitmix64 of the id's two 64-bit halves XORed, top 32 bits):
//!
//! | id | phase | mod 2 | mod 4 | mod 8 |
//! |---|---|---|---|---|
//! | 0 | 1535705161 | 1 | 1 | 1 |
//! | 1 | 312050521 | 1 | 1 | 1 |
//! | 2 | 2223718856 | 0 | 0 | 0 |
//! | 3 | 2261354152 | 0 | 0 | 0 |
//! | 4 | 4065596716 | 0 | 0 | 4 |
//! | 5 | 2543959134 | 0 | 2 | 6 |
//!
//! An entity at rate r is due at tick t when (t + phase) mod r = 0, on its first tick, and on any
//! tick its level changes. Tests run at 64 Hz so every step is an exact binary fraction.

mod common;

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use common::{Pos, ids};
use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;
use concordia_core::time::TickRate;
use concordia_reflect::Reflect;
use concordia_terra::{
    LodProfile, LodViewer, Phase, Schedule, SimClock, SimLod, SimLoop, SystemCtx, TimeScale, View,
    World, WorldHash, is_due, lod_phase, lod_stats, lod_system, processor, system,
};
use serde::{Deserialize, Serialize};

const HZ_64: TickRate = TickRate::new(std::num::NonZeroU32::new(64).unwrap());
const DT: f64 = 1.0 / 64.0;
const CHUNK: NonZeroUsize = NonZeroUsize::new(1024).unwrap();

/// A test signal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Ping(pub u64);

/// Accumulates what a LOD processor saw (hashed).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct Acc {
    /// Visits.
    pub visits: u32,
    /// Summed elapsed time.
    pub time: f64,
}

/// A stand-in for a render camera: presentation-only, never hashed.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct Camera {
    /// X.
    pub x: f32,
}

fn jobs(threads: usize) -> JobSystem {
    JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap()
}

fn position(world: &World, id: EntityId) -> Option<[f32; 3]> {
    world.get::<Pos>(id).map(|p| [p.x, p.y, p.z])
}

fn profile() -> LodProfile {
    LodProfile::new([10.0, 20.0, 40.0], [2, 4, 8], 2.0).unwrap()
}

fn at(x: f32) -> Pos {
    Pos { x, y: 0.0, z: 0.0 }
}

/// A world with a viewer (ids[0]) at the origin, if `viewer`.
fn world_with_viewer(viewer: bool) -> World {
    let mut world = World::new();
    world.register_shared::<LodProfile>().unwrap();
    let v = ids(1)[0];
    world.spawn_with_id(v).unwrap();
    world.insert(v, at(0.0)).unwrap();
    if viewer {
        world.insert(v, LodViewer).unwrap();
    }
    world
}

fn add(world: &mut World, id: EntityId, x: f32, profiled: bool) {
    world.spawn_with_id(id).unwrap();
    world.insert(id, at(x)).unwrap();
    world.insert(id, Acc::default()).unwrap();
    if profiled {
        world.insert(id, profile()).unwrap();
    }
}

type Visits = Arc<Mutex<Vec<(u64, EntityId, f64, u8)>>>;

/// `terra.lod` first in `PrePhysics`, then a LOD-aware processor logging (tick, id, dt, level).
fn logging_schedule(log: Visits) -> Schedule {
    let mut schedule = Schedule::new();
    schedule.add_in(Phase::PrePhysics, lod_system(position));
    schedule.add_processor(
        processor("visit", Phase::PostPhysics)
            .lod()
            .writes::<Acc>()
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                let log = log.clone();
                view.query::<&mut Acc>()
                    .unwrap()
                    .for_each_due(|id, due, acc| {
                        acc.visits += 1;
                        acc.time += due.dt_exact;
                        assert_eq!(due.dt, due.dt_exact as f32);
                        log.lock()
                            .unwrap()
                            .push((ctx.tick.0, id, due.dt_exact, due.level));
                    });
            }),
    );
    schedule
}

fn visits_of(log: &Visits, id: EntityId) -> Vec<(u64, f64, u8)> {
    let log = log.lock().unwrap();
    log.iter()
        .filter(|v| v.1 == id)
        .map(|v| (v.0, v.2, v.3))
        .collect()
}

fn ticks_of(log: &Visits, id: EntityId) -> Vec<u64> {
    visits_of(log, id).into_iter().map(|v| v.0).collect()
}

#[test]
fn lod_phase_is_a_fixed_hash_of_the_id() {
    let ids = ids(6);
    let expected = [
        1_535_705_161u32,
        312_050_521,
        2_223_718_856,
        2_261_354_152,
        4_065_596_716,
        2_543_959_134,
    ];
    for (id, phase) in ids.iter().zip(expected) {
        assert_eq!(lod_phase(*id), phase, "{id}");
    }
    // (tick + phase) mod rate = 0.
    assert!(is_due(4, 4_065_596_716, 8));
    assert!(!is_due(5, 4_065_596_716, 8));
    assert!(is_due(2, 2_543_959_134, 4));
    assert!(!is_due(0, 2_543_959_134, 4));
    assert!(is_due(7, 3, 1));
    // No overflow at the ends of the ranges.
    assert!(is_due(u64::MAX, 1, 1));
    assert!(!is_due(u64::MAX - 3, u32::MAX, 4));
    assert!(is_due(u64::MAX - 2, u32::MAX, 4));
}

#[test]
fn far_entities_tick_at_their_lod_rate() {
    let ids = ids(7);
    let mut world = world_with_viewer(true);
    add(&mut world, ids[1], 5.0, true); // level 0
    add(&mut world, ids[2], 15.0, true); // level 1, rate 2, phase ≡ 0
    add(&mut world, ids[3], 30.0, true); // level 2, rate 4, phase ≡ 0
    add(&mut world, ids[5], 30.0, true); // level 2, rate 4, phase ≡ 2
    add(&mut world, ids[4], 50.0, true); // level 3, rate 8, phase ≡ 4
    add(&mut world, ids[6], 50.0, false); // no profile: every tick
    let log: Visits = Arc::default();
    let mut sim = SimLoop::new(HZ_64, logging_schedule(log.clone()), &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 16);

    let every: Vec<u64> = (0..16).collect();
    assert_eq!(ticks_of(&log, ids[1]), every);
    assert_eq!(ticks_of(&log, ids[2]), [0, 2, 4, 6, 8, 10, 12, 14]);
    assert_eq!(ticks_of(&log, ids[3]), [0, 4, 8, 12]);
    assert_eq!(ticks_of(&log, ids[5]), [0, 2, 6, 10, 14]);
    assert_eq!(ticks_of(&log, ids[4]), [0, 4, 12]);
    assert_eq!(ticks_of(&log, ids[6]), every);
    // The viewer has no profile and no `Acc`: never visited by the query.
    assert!(ticks_of(&log, ids[0]).is_empty());

    // Elapsed: the first tick covers one step; later ones everything since the last.
    assert_eq!(
        visits_of(&log, ids[4]),
        [(0, DT, 3), (4, 4.0 * DT, 3), (12, 8.0 * DT, 3)]
    );
    assert_eq!(
        visits_of(&log, ids[5]),
        [
            (0, DT, 2),
            (2, 2.0 * DT, 2),
            (6, 4.0 * DT, 2),
            (10, 4.0 * DT, 2),
            (14, 4.0 * DT, 2)
        ]
    );
    // No profile: level 0 and the tick's dt.
    assert!(
        visits_of(&log, ids[6])
            .iter()
            .all(|v| v.1 == DT && v.2 == 0)
    );

    let lod = |id| *world.get::<SimLod>(id).unwrap();
    assert_eq!(lod(ids[4]).level, 3);
    assert_eq!(lod(ids[4]).last_time, 13.0 * DT);
    assert_eq!(lod(ids[5]).last_time, 15.0 * DT);
    assert_eq!(lod(ids[1]).level, 0);
    assert!(
        world.get::<SimLod>(ids[6]).is_none(),
        "no profile, no SimLod"
    );
    assert!(
        world.get::<SimLod>(ids[0]).is_none(),
        "the viewer has no profile"
    );
    // Tick 15: ids[1] and ids[2]? ids[2] is due on even ticks only.
    let stats = lod_stats(&world);
    assert_eq!(stats.viewers, 1);
    assert_eq!(stats.levels, [1, 1, 2, 1]);
    assert_eq!(stats.due, 1, "at tick 15 only the level-0 entity is due");
}

#[test]
fn lod_transitions_are_hysteresis_damped() {
    let id = ids(3)[2]; // phase ≡ 0 mod 2, 4 and 8
    let mut world = world_with_viewer(true);
    add(&mut world, id, 9.0, true);
    let log: Visits = Arc::default();
    let mut sim = SimLoop::new(HZ_64, logging_schedule(log), &mut world).unwrap();
    // (distance before the tick, level after it, due); boundaries 10, 20, 40, hysteresis 2.
    let table: [(f32, u8, bool); 15] = [
        (9.0, 0, true),   // 0: first tick
        (10.0, 1, true),  // 1: a boundary belongs to the far side; changed
        (9.0, 1, true),   // 2: inside 10 but not 8: stays; due (2 mod 2)
        (8.0, 1, false),  // 3: exactly 10 − 2 still stays; 3 is odd
        (7.9, 0, true),   // 4: inside 8: back to 0; changed
        (10.0, 1, true),  // 5: changed (5 is odd)
        (20.0, 2, true),  // 6: changed
        (45.0, 3, true),  // 7: changed
        (38.5, 3, true),  // 8: inside 40 but not 38: stays; due (8 mod 8)
        (38.0, 3, false), // 9: exactly 38 stays; 9 mod 8 ≠ 0
        (37.0, 2, true),  // 10: changed
        (17.0, 1, true),  // 11: inside 18: level 1 (not 0: 17 ≥ 8); changed
        (5.0, 0, true),   // 12: changed
        (45.0, 3, true),  // 13: losing detail jumps straight to 3
        (5.0, 0, true),   // 14: and gaining it straight to 0
    ];
    for (tick, (distance, level, due)) in table.into_iter().enumerate() {
        world.get_mut::<Pos>(id).unwrap().x = distance;
        sim.step(&mut world, &jobs(1), 1);
        let lod = world.get::<SimLod>(id).unwrap();
        assert_eq!(
            (lod.level, lod.due),
            (level, due),
            "tick {tick} at {distance} m"
        );
    }
}

#[test]
fn level_rule_uses_squared_distances_with_a_margin() {
    let p = profile();
    // Coming from level 0: thresholds 10, 20, 40 (inclusive).
    assert_eq!(p.level(0, 99.99), 0);
    assert_eq!(p.level(0, 100.0), 1);
    assert_eq!(p.level(0, 400.0), 2);
    assert_eq!(p.level(0, 1600.0), 3);
    // Going back: inside 8, 18, 38 only.
    assert_eq!(p.level(3, 38.0 * 38.0), 3);
    assert_eq!(p.level(3, 37.9 * 37.9), 2);
    assert_eq!(p.level(2, 18.0 * 18.0), 2);
    assert_eq!(p.level(2, 17.9 * 17.9), 1);
    assert_eq!(p.level(1, 64.0), 1);
    assert_eq!(p.level(1, 63.9), 0);
    assert_eq!(p.level(3, 0.0), 0);
    assert_eq!(p.rate(0), 1);
    assert_eq!([p.rate(1), p.rate(2), p.rate(3)], [2, 4, 8]);
}

#[test]
fn malformed_profiles_are_descriptive_errors() {
    let bad = [
        LodProfile::new([10.0, 5.0, 40.0], [2, 4, 8], 1.0),
        LodProfile::new([0.0, 20.0, 40.0], [2, 4, 8], 1.0),
        LodProfile::new([10.0, f32::NAN, 40.0], [2, 4, 8], 1.0),
        LodProfile::new([10.0, 20.0, 40.0], [0, 4, 8], 1.0),
        LodProfile::new([10.0, 20.0, 40.0], [4, 2, 8], 1.0),
        LodProfile::new([10.0, 20.0, 40.0], [2, 4, 8], -1.0),
        LodProfile::new([10.0, 20.0, 40.0], [2, 4, 8], f32::INFINITY),
        // A margin as wide as the first distance would never give detail back.
        LodProfile::new([2.0, 20.0, 40.0], [2, 4, 8], 2.0),
    ];
    for (i, result) in bad.into_iter().enumerate() {
        let message = result.expect_err(&format!("case {i}")).to_string();
        assert!(message.contains("LOD profile"), "case {i}: {message}");
    }
    // Equal distances and rates are allowed (a level can be skipped).
    assert!(LodProfile::new([10.0, 10.0, 40.0], [2, 2, 8], 0.0).is_ok());
}

#[test]
fn invalid_profiles_in_the_world_mean_full_detail() {
    let id = ids(2)[1];
    let mut world = world_with_viewer(true);
    add(&mut world, id, 50.0, false);
    // Bypassing `new` (as a hand-edited scene could).
    let broken = LodProfile {
        distances: [40.0, 20.0, 10.0],
        rates: [2, 4, 8],
        hysteresis: 1.0,
    };
    world.insert(id, broken).unwrap();
    let log: Visits = Arc::default();
    let mut sim = SimLoop::new(HZ_64, logging_schedule(log.clone()), &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 4);
    assert_eq!(ticks_of(&log, id), [0, 1, 2, 3]);
    assert_eq!(world.get::<SimLod>(id).unwrap().level, 0);
}

/// Builds the same entities in `order`, with a presentation-only camera and chunk data in `noisy`.
fn insertion_world(order: &[usize], noisy: bool) -> World {
    let ids = ids(12);
    let mut world = world_with_viewer(true);
    world.register_presentation::<Camera>();
    for &i in order {
        add(&mut world, ids[i], 4.0 * i as f32, true);
        if noisy && i % 3 == 0 {
            // Archetype churn changes row layout, not state.
            world.insert(ids[i], Camera { x: 1.0 }).unwrap();
            world.remove::<Camera>(ids[i]).unwrap();
        }
    }
    // The camera's holder exists in both worlds; only the noisy one gives it a camera.
    world.spawn_with_id(ids[11]).unwrap();
    if noisy {
        world.insert(ids[11], Camera { x: 0.0 }).unwrap();
        world.set_chunk(ids[2], Camera { x: 9.0 }).unwrap();
    }
    world
}

#[test]
fn simulation_lod_depends_only_on_deterministic_inputs() {
    let forward: Vec<usize> = (1..11).collect();
    let backward: Vec<usize> = (1..11).rev().collect();
    let viewer = ids(1)[0];
    let run = |order: &[usize], noisy: bool, path: fn(u64) -> f32| {
        let mut world = insertion_world(order, noisy);
        let log: Visits = Arc::default();
        let mut sim = SimLoop::new(HZ_64, logging_schedule(log.clone()), &mut world).unwrap();
        let mut levels = Vec::new();
        for tick in 0..40u64 {
            world.get_mut::<Pos>(viewer).unwrap().x = path(tick);
            if noisy {
                // The "camera" moves somewhere else entirely; it is not a viewer.
                world.get_mut::<Camera>(ids(12)[11]).unwrap().x = -(tick as f32) * 10.0;
            }
            sim.step(&mut world, &jobs(1), 1);
            let mut row: Vec<(EntityId, SimLod)> = world
                .entity_ids_sorted()
                .into_iter()
                .filter_map(|id| world.get::<SimLod>(id).map(|l| (id, *l)))
                .collect();
            row.sort_by_key(|r| r.0);
            levels.push(row);
        }
        let mut sorted = log.lock().unwrap().clone();
        sorted.sort_by_key(|a| (a.0, a.1));
        (levels, sorted, world.state_hash().unwrap())
    };
    let sweep: fn(u64) -> f32 = |t| t as f32 * 2.0 - 20.0;
    let (levels, visits, hash) = run(&forward, false, sweep);
    let distinct: std::collections::BTreeSet<u8> =
        levels.iter().flatten().map(|(_, l)| l.level).collect();
    assert_eq!(distinct.len(), 4, "the sweep visits every level");
    let (levels2, visits2, hash2): (_, _, WorldHash) = run(&backward, true, sweep);
    assert_eq!(
        levels, levels2,
        "insertion order, layout, chunk data and the camera don't matter"
    );
    assert_eq!(visits, visits2);
    assert_eq!(hash, hash2);
    // The viewer does matter.
    let (levels3, _, hash3) = run(&forward, false, |t| t as f32 * 3.0 - 20.0);
    assert_ne!(levels, levels3);
    assert_ne!(hash, hash3);
}

#[test]
fn signals_survive_lod_changes() {
    let ids = ids(5);
    let target = ids[4]; // phase ≡ 4 (mod 8), 0 (mod 2)
    let mut world = world_with_viewer(true);
    add(&mut world, target, 50.0, true);
    let seen: Arc<Mutex<Vec<(u64, u64, bool)>>> = Arc::default();
    let visits: Arc<Mutex<Vec<u64>>> = Arc::default();
    let mut schedule = Schedule::new();
    schedule.add_in(Phase::PrePhysics, lod_system(position));
    schedule.add_in(
        Phase::PostPhysics,
        system("raise", move |world: &mut World, ctx: &SystemCtx<'_>| {
            if [1, 6, 8].contains(&ctx.tick.0) {
                world.signal(target, Ping(ctx.tick.0));
            }
        }),
    );
    let (s, v) = (seen.clone(), visits.clone());
    schedule.add_processor(
        processor("receive", Phase::PostPhysics)
            .lod()
            .receives::<Ping>()
            .reads::<Pos>()
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                let delivered: Vec<(EntityId, Ping)> = view.signals::<Ping>().unwrap().to_vec();
                let targets: Vec<EntityId> = delivered.iter().map(|(id, _)| *id).collect();
                let mut reached = Vec::new();
                view.query::<&Pos>()
                    .unwrap()
                    .for_each_of(&targets, |id, _| reached.push(id));
                for (id, ping) in delivered {
                    s.lock()
                        .unwrap()
                        .push((ctx.tick.0, ping.0, reached.contains(&id)));
                }
                view.query::<&Pos>().unwrap().for_each(|id, _| {
                    if id == target {
                        v.lock().unwrap().push(ctx.tick.0);
                    }
                });
            }),
    );
    let mut sim = SimLoop::new(HZ_64, schedule, &mut world).unwrap();
    for tick in 0..13u64 {
        match tick {
            6 => world.get_mut::<Pos>(target).unwrap().x = 15.0, // level 1 on the raise tick
            9 => world.get_mut::<Pos>(target).unwrap().x = 50.0, // level 3 on the delivery tick
            _ => {}
        }
        sim.step(&mut world, &jobs(1), 1);
    }
    // Delivered the tick after each raise, and reachable, due or not (2 and 7 aren't due).
    assert_eq!(
        *seen.lock().unwrap(),
        [(2, 1, true), (7, 6, true), (9, 8, true)]
    );
    // Its regular visits follow its LOD: first tick, 4 (level 3), 6 (changed), 8 (level 1),
    // 9 (changed), 12 (level 3).
    assert_eq!(*visits.lock().unwrap(), [0, 4, 6, 8, 9, 12]);
}

#[test]
fn no_viewer_means_full_detail() {
    let ids = ids(5);
    let mut world = world_with_viewer(false);
    for (k, &id) in ids[1..].iter().enumerate() {
        add(&mut world, id, 100.0 + k as f32 * 50.0, true);
    }
    let log: Visits = Arc::default();
    let mut sim = SimLoop::new(HZ_64, logging_schedule(log.clone()), &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 8);
    for &id in &ids[1..] {
        let expected: Vec<(u64, f64, u8)> = (0..8).map(|t| (t, DT, 0)).collect();
        assert_eq!(visits_of(&log, id), expected, "{id}");
        assert_eq!(world.get::<SimLod>(id).unwrap().level, 0);
    }
    // A viewer appears: far entities lose detail on the next tick.
    world.insert(ids[0], LodViewer).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    assert_eq!(world.get::<SimLod>(ids[1]).unwrap().level, 3);
    // It goes: full detail again, due at once.
    world.despawn(ids[0]).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    for &id in &ids[1..] {
        let lod = world.get::<SimLod>(id).unwrap();
        assert_eq!((lod.level, lod.due), (0, true), "{id}");
    }
    // Entities that lose their profile lose their SimLod.
    world.remove::<LodProfile>(ids[1]).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    assert!(world.get::<SimLod>(ids[1]).is_none());
}

#[test]
fn lod_processors_get_exact_elapsed_time_under_time_scale_changes() {
    let ids = ids(4);
    let mut world = world_with_viewer(true);
    add(&mut world, ids[1], 5.0, true); // level 0
    add(&mut world, ids[3], 30.0, true); // level 2, rate 4, phase ≡ 0: due 0, 4, 8, 12, 16
    let log: Visits = Arc::default();
    let mut sim = SimLoop::new(HZ_64, logging_schedule(log.clone()), &mut world).unwrap();
    let scale = |tick: u64| match tick {
        0..=1 => 1.0,
        2..=5 => 0.5,
        6..=9 => 0.0,
        10..=13 => 2.0,
        _ => 1.0,
    };
    for tick in 0..17u64 {
        world.resource_mut::<SimClock>().unwrap().time_scale = TimeScale::new(scale(tick)).unwrap();
        sim.step(&mut world, &jobs(1), 1);
    }
    let h = DT / 2.0;
    // Ticks 1–4: 1 + 0.5 × 3 steps; 5–8: 0.5 + 0; 9–12: 0 + 2 × 3; 13–16: 2 + 1 × 3.
    assert_eq!(
        visits_of(&log, ids[3]),
        [
            (0, DT, 2),
            (4, 5.0 * h, 2),
            (8, h, 2),
            (12, 12.0 * h, 2),
            (16, 10.0 * h, 2)
        ]
    );
    let full: Vec<f64> = visits_of(&log, ids[1]).into_iter().map(|v| v.1).collect();
    let expected: Vec<f64> = (0..17).map(|t| f64::from(scale(t)) * DT).collect();
    assert_eq!(full, expected);
    let clock = *world.resource::<SimClock>().unwrap();
    assert_eq!(clock.time, 30.0 * h);
    for id in [ids[1], ids[3]] {
        assert_eq!(world.get::<Acc>(id).unwrap().time, clock.time, "{id}");
        assert_eq!(world.get::<SimLod>(id).unwrap().last_time, clock.time);
    }
}

#[test]
fn non_lod_processors_still_run_every_tick() {
    let ids = ids(5);
    let mut world = world_with_viewer(true);
    for (k, &id) in ids[1..].iter().enumerate() {
        add(&mut world, id, 5.0 + 15.0 * k as f32, true);
    }
    let all: Visits = Arc::default();
    let lod: Visits = Arc::default();
    let mut schedule = logging_schedule(lod.clone());
    let log = all.clone();
    // It reads `SimLod` too (as a debug view might), and still sees every entity.
    schedule.add_processor(
        processor("every", Phase::PostPhysics)
            .reads::<Acc>()
            .reads::<SimLod>()
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                view.query::<(&Acc, &SimLod)>().unwrap().par_for_each_due(
                    ctx.jobs,
                    CHUNK,
                    |id, due, _| {
                        log.lock()
                            .unwrap()
                            .push((ctx.tick.0, id, due.dt_exact, due.level));
                    },
                );
            }),
    );
    let mut sim = SimLoop::new(HZ_64, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(4), 16);
    for &id in &ids[1..] {
        let expected: Vec<(u64, f64, u8)> = (0..16).map(|t| (t, DT, 0)).collect();
        let mut got = visits_of(&all, id);
        got.sort_by_key(|v| v.0);
        assert_eq!(got, expected, "{id}");
    }
    // The LOD processor beside it skipped the far ones.
    assert_eq!(ticks_of(&lod, ids[4]).len(), 3, "level 3: ticks 0, 4, 12");
}

#[test]
fn lod_is_deterministic_across_thread_counts() {
    const SIDE: usize = 317; // 100,489 entities
    let run = |threads: usize| {
        let mut world = World::new();
        world.register_shared::<LodProfile>().unwrap();
        let profile = LodProfile::new([20.0, 60.0, 120.0], [2, 4, 8], 3.0).unwrap();
        let mut source = concordia_core::id::DeterministicIdGen::new(7);
        let viewers: Vec<EntityId> = (0..2)
            .map(|k| {
                let id = concordia_core::id::IdSource::next_id(&mut source).unwrap();
                world.spawn_with_id(id).unwrap();
                world.insert(id, at(k as f32 * 200.0)).unwrap();
                world.insert(id, LodViewer).unwrap();
                id
            })
            .collect();
        for i in 0..SIDE * SIDE {
            let id = concordia_core::id::IdSource::next_id(&mut source).unwrap();
            world.spawn_with_id(id).unwrap();
            let (x, z) = ((i % SIDE) as f32, (i / SIDE) as f32);
            world.insert(id, Pos { x, y: 0.0, z }).unwrap();
            world.insert(id, Acc::default()).unwrap();
            world.insert(id, profile).unwrap();
        }
        let mut schedule = Schedule::new();
        schedule.add_in(Phase::PrePhysics, lod_system(position));
        schedule.add_in(
            Phase::PrePhysics,
            system("orbit", move |world: &mut World, ctx: &SystemCtx<'_>| {
                let t = ctx.tick.0 as f32;
                for (k, &v) in viewers.iter().enumerate() {
                    let p = world.get_mut::<Pos>(v).unwrap();
                    p.x = 150.0 + (t * 0.5) * if k == 0 { 1.0 } else { -1.0 };
                    p.z = 150.0 + t * 0.25;
                }
            }),
        );
        schedule.add_processor(
            processor("integrate", Phase::PostPhysics)
                .lod()
                .writes::<Acc>()
                .run(|view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                    view.query::<&mut Acc>().unwrap().par_for_each_due(
                        ctx.jobs,
                        CHUNK,
                        |_, due, acc| {
                            acc.visits += 1;
                            acc.time += due.dt_exact * f64::from(due.level + 1);
                        },
                    );
                }),
        );
        let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
        let jobs = jobs(threads);
        let mut hashes = Vec::new();
        let mut last = 0;
        for at_tick in [1u32, 60, 300] {
            sim.step(&mut world, &jobs, at_tick - last);
            last = at_tick;
            hashes.push(world.state_hash().unwrap());
        }
        (hashes, lod_stats(&world))
    };
    let (one, stats) = run(1);
    assert_eq!(run(8), (one, stats));
    assert_eq!(stats.viewers, 2);
    assert_eq!(stats.levels.iter().sum::<u64>(), (SIDE * SIDE) as u64);
    assert!(stats.levels.iter().all(|&n| n > 1000), "{stats:?}");
    assert!(stats.due < (SIDE * SIDE) as u64 / 2, "{stats:?}");
}

#[test]
fn the_nearest_viewer_sets_the_level() {
    let ids = ids(4);
    let mut world = world_with_viewer(true); // ids[0] at 0
    world.spawn_with_id(ids[1]).unwrap();
    world.insert(ids[1], at(100.0)).unwrap();
    world.insert(ids[1], LodViewer).unwrap();
    add(&mut world, ids[2], 95.0, true); // 5 m from ids[1], 95 m from ids[0]
    add(&mut world, ids[3], 50.0, true); // 50 m from both
    let log: Visits = Arc::default();
    let mut sim = SimLoop::new(HZ_64, logging_schedule(log), &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    assert_eq!(world.get::<SimLod>(ids[2]).unwrap().level, 0);
    assert_eq!(world.get::<SimLod>(ids[3]).unwrap().level, 3);
    assert_eq!(lod_stats(&world).viewers, 2);
}
