//! Resource access in processor views (M2 phase 2.0b, ADR-0069): declared like components, and
//! part of the conflict graph.

mod common;

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Pos, moving_world};
use concordia_core::jobs::JobSystem;
use concordia_core::rng::SimRng;
use concordia_core::time::TickRate;
use concordia_terra::{
    Phase, QueryError, Schedule, ScheduleError, SimLoop, SystemCtx, View, World, processor,
};

fn jobs(threads: usize) -> JobSystem {
    JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap()
}

/// A counter resource.
#[derive(Debug, Clone, Default, PartialEq)]
struct Score(u32);

#[test]
fn processors_read_and_write_declared_resources() {
    let mut schedule = Schedule::new();
    schedule.add_processor(
        processor("count", Phase::PostPhysics)
            .reads::<Pos>()
            .writes_resource::<Score>()
            .run(|view: &mut View<'_>, _: &SystemCtx<'_>| {
                let mut n = 0;
                view.query::<&Pos>().unwrap().for_each(|_, _| n += 1);
                view.resource_mut::<Score>().unwrap().0 += n;
            }),
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    let out = seen.clone();
    schedule.add_processor(
        processor("report", Phase::FrameEnd)
            .reads_resource::<Score>()
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                out.lock()
                    .unwrap()
                    .push(view.resource::<Score>().unwrap().0);
            }),
    );
    let mut world = moving_world(3);
    world.insert_resource(Score::default());
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(4), 2);
    assert_eq!(*seen.lock().unwrap(), [3, 6]);
    assert_eq!(
        world.resource::<Score>(),
        Some(&Score(6)),
        "put back after each wave"
    );
}

#[test]
fn resource_writers_never_share_a_wave() {
    let in_flight = Arc::new(AtomicUsize::new(0));
    let overlap = Arc::new(AtomicUsize::new(0));
    let mut schedule = Schedule::new();
    for name in ["roll_a", "roll_b"] {
        let (in_flight, overlap) = (in_flight.clone(), overlap.clone());
        schedule.add_processor(
            processor(name, Phase::PostPhysics)
                .writes_resource::<SimRng>()
                .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                    if in_flight.fetch_add(1, Ordering::SeqCst) > 0 {
                        overlap.fetch_add(1, Ordering::SeqCst);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                    let _ = view.resource_mut::<SimRng>().unwrap().next_u64();
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                }),
        );
    }
    let mut world = World::new();
    world.insert_resource(SimRng::new(7));
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(8), 1);
    assert_eq!(overlap.load(Ordering::SeqCst), 0);
}

#[test]
fn undeclared_or_missing_resources_are_errors() {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let out = errors.clone();
    let mut schedule = Schedule::new();
    schedule.add_processor(
        processor("nosy", Phase::PostPhysics)
            .reads_resource::<Score>()
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                let mut e = out.lock().unwrap();
                e.push(view.resource::<SimRng>().map(|_| ()).unwrap_err());
                e.push(view.resource_mut::<Score>().map(|_| ()).unwrap_err());
                e.push(view.resource::<Score>().map(|_| ()).unwrap_err());
            }),
    );
    let mut world = World::new();
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    let e = errors.lock().unwrap();
    assert!(matches!(e[0], QueryError::Undeclared { .. }), "{:?}", e[0]);
    assert!(matches!(e[1], QueryError::ReadOnly { .. }), "{:?}", e[1]);
    assert!(
        matches!(e[2], QueryError::MissingResource { .. }),
        "{:?}",
        e[2]
    );
}

#[test]
fn presentation_processors_may_not_write_simulation_resources() {
    let mut schedule = Schedule::new();
    schedule.add_processor(
        processor("draw", Phase::Presentation)
            .writes_resource::<Score>()
            .run(|_: &mut View<'_>, _: &SystemCtx<'_>| {}),
    );
    let mut world = World::new();
    assert!(matches!(
        SimLoop::new(TickRate::HZ_60, schedule, &mut world),
        Err(ScheduleError::PresentationWritesSimulation { .. })
    ));
}
