//! Clip events (2.4.4): which events a step crosses (hand-computed), and how they arrive as Terra
//! signals through `SimLoop` (next tick, entity order, crossing order within an entity).

mod common;

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use common::{clip, joint};
use concordia_anim::{
    AnimEvent, AnimLibrary, AnimPlayer, BoundClip, Crossing, Skeleton, add_processors,
    crossed_events, insert_rest_poses,
};
use concordia_assets::{AssetRef, ChannelPath, ClipEvent, Interpolation, Skinned};
use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;
use concordia_core::math::Vec3;
use concordia_core::time::TickRate;
use concordia_terra::{
    Phase, Schedule, SimClock, SimLoop, SystemCtx, TimeScale, View, World, processor,
};

const FILE: &str = "018bcfe5-6800-7000-a424-6c55076b137f";
const EPS: f32 = 1e-6;

fn ev(time: f32, name: &str) -> ClipEvent {
    ClipEvent {
        time,
        name: name.to_owned(),
    }
}

/// start 0, a 0.25, b 0.5, end 1.0, in a 1 s clip.
fn marks() -> Vec<ClipEvent> {
    vec![
        ev(0.0, "start"),
        ev(0.25, "a"),
        ev(0.5, "b"),
        ev(1.0, "end"),
    ]
}

/// The crossed events' names and step fractions.
fn crossed(looping: bool, time: f32, delta: f32) -> Vec<(&'static str, f32)> {
    const NAMES: [&str; 4] = ["start", "a", "b", "end"];
    crossed_events(&marks(), 1.0, looping, time, delta)
        .into_iter()
        .map(|Crossing { event, at }| (NAMES[event], at))
        .collect()
}

fn assert_crossed(got: Vec<(&str, f32)>, want: &[(&str, f32)], what: &str) {
    assert_eq!(
        got.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        want.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        "{what}: {got:?}"
    );
    for ((_, a), (_, b)) in got.iter().zip(want) {
        assert!((a - b).abs() < EPS, "{what}: fraction {a} vs {b}");
    }
}

#[test]
fn animation_events_fire_once_per_crossing_including_loops_and_large_steps() {
    // Forward steps cover [from, to): an event at the start fires when playback starts.
    assert_crossed(crossed(true, 0.0, 0.25), &[("start", 0.0)], "start");
    assert_crossed(
        crossed(true, 0.25, 0.25),
        &[("a", 0.0)],
        "the step starting on a",
    );
    assert_crossed(crossed(true, 0.3, 0.1), &[], "nothing between keys");
    // A wrap: end (lap 0) then start (lap 1), both half-way through the step.
    assert_crossed(
        crossed(true, 0.75, 0.5),
        &[("end", 0.5), ("start", 0.5)],
        "wrap",
    );
    // Landing exactly on the end: nothing yet; the next step (from 0) fires start and end's lap.
    assert_crossed(crossed(true, 0.75, 0.25), &[], "landing on the wrap");
    assert_crossed(
        crossed(true, 0.0, 0.25),
        &[("start", 0.0)],
        "after landing on the wrap, start fires once",
    );
    // A step of 2.5 laps from 0.1 fires every crossing once per lap, in order.
    let at = |x: f32| (x - 0.1) / 2.5;
    assert_crossed(
        crossed(true, 0.1, 2.5),
        &[
            ("a", at(0.25)),
            ("b", at(0.5)),
            ("end", at(1.0)),
            ("start", at(1.0)),
            ("a", at(1.25)),
            ("b", at(1.5)),
            ("end", at(2.0)),
            ("start", at(2.0)),
            ("a", at(2.25)),
            ("b", at(2.5)),
        ],
        "large step",
    );
    // Clamped clips cover their end when they reach it, then fire nothing while held.
    assert_crossed(crossed(false, 0.75, 0.5), &[("end", 0.5)], "clamped end");
    assert_crossed(
        crossed(false, 0.75, 0.25),
        &[("end", 1.0)],
        "landing on the end",
    );
    assert_crossed(crossed(false, 1.0, 0.25), &[], "held at the end");
    assert_crossed(
        crossed(false, 0.1, 5.0),
        &[("a", 0.03), ("b", 0.08), ("end", 0.18)],
        "huge clamped step",
    );
    // Backward steps cover (to, from]: mirror images.
    assert_crossed(crossed(true, 0.5, -0.25), &[("b", 0.0)], "backward");
    assert_crossed(
        crossed(true, 0.25, -0.5),
        &[("a", 0.0), ("start", 0.5), ("end", 0.5)],
        "backward wrap",
    );
    assert_crossed(
        crossed(false, 0.25, -0.5),
        &[("a", 0.0), ("start", 0.5)],
        "backward to the clamped start",
    );
    assert_crossed(crossed(false, 0.0, -0.25), &[], "held at the start");
    // Zero, non-finite and empty cases fire nothing, even sitting on an event.
    assert_crossed(crossed(true, 0.0, 0.0), &[], "time scale 0");
    assert_crossed(crossed(true, 0.25, 0.0), &[], "speed 0 on an event");
    assert_crossed(crossed(true, 0.0, f32::NAN), &[], "NaN step");
    assert!(
        crossed_events(&marks(), 0.0, true, 0.0, 0.25).is_empty(),
        "empty clip"
    );
    // Two events at one time keep their authored order.
    let twins = [ev(0.5, "first"), ev(0.5, "second")];
    let got: Vec<usize> = crossed_events(&twins, 1.0, true, 0.4, 0.2)
        .iter()
        .map(|c| c.event)
        .collect();
    assert_eq!(got, [0, 1]);
}

/// Ticks of 1/8 s.
fn rate() -> TickRate {
    TickRate::from_hz(8).unwrap()
}

fn skin() -> AssetRef {
    format!("asset:{FILE}#skin/0").parse().unwrap()
}

fn clip_ref(name: &str) -> AssetRef {
    format!("asset:{FILE}#clip/{name}").parse().unwrap()
}

/// A two-bone library with a 1 s looping clip "steps": events at 0.25 ("left") and 0.3 ("right").
fn library() -> AnimLibrary {
    let skeleton = Skeleton::new(vec![
        joint("hip", None, Vec3::ZERO),
        joint("arm", Some(0), Vec3::Y),
    ])
    .unwrap();
    let mut data = clip(
        "hip",
        ChannelPath::Translation,
        Interpolation::Linear,
        &[0.0, 1.0],
        &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    );
    data.name = "steps".into();
    data.events = vec![ev(0.25, "left"), ev(0.3, "right")];
    let bound = BoundClip::bind(&data, &skeleton).unwrap();
    let mut library = AnimLibrary::new();
    library.insert_clip(clip_ref("steps"), skin(), bound);
    library.insert_skeleton(skin(), skeleton);
    library
}

/// (tick it arrived on, target, event name, weight).
type Log = Arc<Mutex<Vec<(u32, EntityId, String, f32)>>>;

/// A `PrePhysics` processor that logs every delivered `AnimEvent` with the tick number.
fn receiver(log: Log) -> impl concordia_terra::Processor {
    let tick = Mutex::new(0u32);
    processor("test.receive", Phase::PrePhysics)
        .receives::<AnimEvent>()
        .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
            let mut tick = tick.lock().unwrap();
            *tick += 1;
            for (id, e) in view.signals::<AnimEvent>().unwrap() {
                log.lock()
                    .unwrap()
                    .push((*tick, *id, e.name.clone(), e.weight));
            }
        })
}

#[test]
fn events_arrive_as_signals_next_tick_in_entity_order() {
    let library = Arc::new(library());
    let mut world = World::new();
    // Spawned high id first, so spawn order isn't entity order.
    let ids: Vec<EntityId> = [
        "018bcfe5-6800-7000-a424-0000000000c0",
        "018bcfe5-6800-7000-a424-0000000000a0",
        "018bcfe5-6800-7000-a424-0000000000b0",
    ]
    .iter()
    .map(|s| s.parse().unwrap())
    .collect();
    for (id, start) in ids.iter().zip([0.125, 0.125, 0.0]) {
        world.spawn_with_id(*id).unwrap();
        world.insert(*id, Skinned { skin: skin() }).unwrap();
        let mut player = AnimPlayer::new(clip_ref("steps"));
        player.time = start;
        world.insert(*id, player).unwrap();
    }
    insert_rest_poses(&library, &mut world);
    let log: Log = Arc::default();
    let mut schedule = Schedule::new();
    schedule.add_processor(receiver(log.clone()));
    add_processors(&mut schedule, library);
    let mut sim = SimLoop::new(rate(), schedule, &mut world).unwrap();
    let jobs = JobSystem::new(NonZeroUsize::new(3).unwrap()).unwrap();

    // Tick 1: the players at 0.125 step over [0.125, 0.25): nothing. Tick 2: [0.25, 0.375)
    // crosses left then right; raised in tick 2's FrameEnd, they arrive at tick 3's start.
    sim.step(&mut world, &jobs, 1);
    assert!(world.pending_signals::<AnimEvent>().is_empty());
    sim.step(&mut world, &jobs, 1);
    let pending: Vec<(EntityId, String)> = world
        .pending_signals::<AnimEvent>()
        .into_iter()
        .map(|(id, e)| (id, e.name.clone()))
        .collect();
    assert_eq!(
        pending,
        [
            (ids[1], "left".to_owned()),
            (ids[1], "right".to_owned()),
            (ids[0], "left".to_owned()),
            (ids[0], "right".to_owned()),
        ],
        "pending, by entity then crossing order"
    );
    assert!(
        log.lock().unwrap().is_empty(),
        "not delivered in the tick that raised them"
    );
    sim.step(&mut world, &jobs, 1);
    let got: Vec<(u32, EntityId, String)> = log
        .lock()
        .unwrap()
        .iter()
        .map(|(t, id, n, _)| (*t, *id, n.clone()))
        .collect();
    assert_eq!(
        got,
        [
            (3, ids[1], "left".to_owned()),
            (3, ids[1], "right".to_owned()),
            (3, ids[0], "left".to_owned()),
            (3, ids[0], "right".to_owned()),
        ],
        "delivered next tick in entity order"
    );
    assert!(
        log.lock().unwrap().iter().all(|e| e.3 == 1.0),
        "clip players fire at weight 1"
    );
    // The third player (from 0) crosses [0.25, 0.375) in tick 3; delivered at tick 4.
    sim.step(&mut world, &jobs, 1);
    let late: Vec<(u32, EntityId)> = log.lock().unwrap()[4..]
        .iter()
        .map(|e| (e.0, e.1))
        .collect();
    assert_eq!(late, [(4, ids[2]), (4, ids[2])]);
    // Once delivered, signals are gone: nothing pending and no repeats until the next lap.
    sim.step(&mut world, &jobs, 2);
    assert_eq!(log.lock().unwrap().len(), 6, "each crossing fires once");

    // Time scale 0 holds playback: no events however long it waits.
    world.resource_mut::<SimClock>().unwrap().time_scale = TimeScale::new(0.0).unwrap();
    let before = log.lock().unwrap().len();
    sim.step(&mut world, &jobs, 40);
    assert_eq!(
        log.lock().unwrap().len(),
        before,
        "frozen playback fires nothing"
    );

    // The payload names the clip and its time; no node for a clip player.
    let (_, first) = (ids[1], log.lock().unwrap()[0].clone());
    assert_eq!(first.2, "left");
}

#[test]
fn event_payload_names_the_clip_and_its_time() {
    let library = Arc::new(library());
    let mut world = World::new();
    let id: EntityId = "018bcfe5-6800-7000-a424-0000000000a0".parse().unwrap();
    world.spawn_with_id(id).unwrap();
    world.insert(id, Skinned { skin: skin() }).unwrap();
    let mut player = AnimPlayer::new(clip_ref("steps"));
    player.time = 0.25;
    world.insert(id, player).unwrap();
    insert_rest_poses(&library, &mut world);
    let mut schedule = Schedule::new();
    add_processors(&mut schedule, library);
    let mut sim = SimLoop::new(rate(), schedule, &mut world).unwrap();
    let jobs = JobSystem::new(NonZeroUsize::MIN).unwrap();
    sim.step(&mut world, &jobs, 1);
    let pending = world.pending_signals::<AnimEvent>();
    assert_eq!(
        pending.len(),
        2,
        "left at 0.25 (the step's start) and right at 0.3"
    );
    assert_eq!(pending[0].0, id);
    assert_eq!(
        *pending[0].1,
        AnimEvent {
            name: "left".into(),
            clip: clip_ref("steps"),
            node: None,
            time: 0.25,
            weight: 1.0,
        }
    );
    // Pending events are hashed (Terra hashes every pending signal's payload).
    let mut other = world.clone();
    other.signal(
        id,
        AnimEvent {
            name: "extra".into(),
            clip: clip_ref("steps"),
            node: None,
            time: 0.0,
            weight: 1.0,
        },
    );
    assert_ne!(world.state_hash().unwrap(), other.state_hash().unwrap());
}

#[test]
fn malformed_events_are_descriptive_errors() {
    let base = || {
        let mut c = clip(
            "hip",
            ChannelPath::Translation,
            Interpolation::Linear,
            &[0.0, 1.0],
            &[0.0; 6],
        );
        c.name = "walk".into();
        c
    };
    for (events, needle) in [
        (vec![ev(1.5, "late")], "late"),
        (vec![ev(-0.1, "early")], "early"),
        (vec![ev(f32::NAN, "nan")], "nan"),
        (vec![ev(0.5, "")], "name"),
        (vec![ev(0.5, "b"), ev(0.25, "a")], "order"),
    ] {
        let mut c = base();
        c.events = events;
        let err = concordia_anim::validate_clip(&c)
            .expect_err(needle)
            .to_string();
        assert!(
            err.contains("walk") && err.contains(needle),
            "{err} lacks {needle}"
        );
    }
    let mut ok = base();
    ok.events = vec![ev(0.0, "a"), ev(0.5, "b"), ev(0.5, "c"), ev(1.0, "d")];
    concordia_anim::validate_clip(&ok).unwrap();
}
