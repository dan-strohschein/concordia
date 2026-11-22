//! Processors, processing phases and deterministic parallel scheduling (M2 phase 2.0, ADR-0063).
//! Mass-style concepts from public documentation only (ADR-0067).

mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::{A, B, Pos, Vel, moving_world};
use concordia_core::jobs::JobSystem;
use concordia_core::time::TickRate;
use concordia_reflect::Reflect;
use concordia_terra::{
    Phase, Schedule, ScheduleError, SimClock, SimLoop, SystemCtx, TimeScale, View, World,
    WorldHash, processor, system,
};
use serde::{Deserialize, Serialize};

const CHUNK: NonZeroUsize = NonZeroUsize::new(256).unwrap();

fn jobs(threads: usize) -> JobSystem {
    JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap()
}

/// Waits until `count` reaches `want` or `limit` passes; true if it got there.
fn wait_for(count: &AtomicUsize, want: usize, limit: Duration) -> bool {
    let start = Instant::now();
    while count.load(Ordering::SeqCst) < want {
        if start.elapsed() > limit {
            return false;
        }
        std::thread::yield_now();
    }
    true
}

fn world_with_a_and_b(n: u32) -> World {
    let mut world = moving_world(n as usize);
    let ids: Vec<_> = world.entity_ids_sorted();
    for (i, id) in ids.into_iter().enumerate() {
        world.insert(id, A(i as u32)).unwrap();
        world.insert(id, B(0.0)).unwrap();
    }
    world
}

#[test]
fn conflicting_processors_never_run_concurrently_and_keep_registration_order() {
    let in_flight = Arc::new(AtomicUsize::new(0));
    let overlap = Arc::new(AtomicUsize::new(0));
    let mut schedule = Schedule::new();
    for (name, factor) in [("double", 2u32), ("add_one", 0)] {
        let (in_flight, overlap) = (in_flight.clone(), overlap.clone());
        schedule.add_processor(processor(name, Phase::PostPhysics).writes::<A>().run(
            move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                if in_flight.fetch_add(1, Ordering::SeqCst) > 0 {
                    overlap.fetch_add(1, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(5));
                view.query::<&mut A>().unwrap().for_each(|_, a| {
                    a.0 = if factor == 0 { a.0 + 1 } else { a.0 * factor };
                });
                in_flight.fetch_sub(1, Ordering::SeqCst);
            },
        ));
    }
    let mut world = world_with_a_and_b(10);
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(8), 1);
    assert_eq!(overlap.load(Ordering::SeqCst), 0);
    let mut values: Vec<u32> = Vec::new();
    world
        .query::<&A>()
        .unwrap()
        .for_each(&mut world, |_, a| values.push(a.0));
    values.sort_unstable();
    // Registration order: double, then add one (i * 2 + 1), never (i + 1) * 2.
    assert_eq!(values, (0..10).map(|i| i * 2 + 1).collect::<Vec<_>>());
}

#[test]
fn independent_processors_run_in_parallel() {
    let started = Arc::new(AtomicUsize::new(0));
    let saw_both = Arc::new(AtomicUsize::new(0));
    let mut schedule = Schedule::new();
    let make = |name: &str, writes_a: bool| {
        let (started, saw_both) = (started.clone(), saw_both.clone());
        let p = processor(name, Phase::PostPhysics);
        let p = if writes_a {
            p.writes::<A>()
        } else {
            p.writes::<B>()
        };
        p.run(move |_: &mut View<'_>, _: &SystemCtx<'_>| {
            started.fetch_add(1, Ordering::SeqCst);
            if wait_for(&started, 2, Duration::from_secs(5)) {
                saw_both.fetch_add(1, Ordering::SeqCst);
            }
        })
    };
    schedule.add_processor(make("on_a", true));
    schedule.add_processor(make("on_b", false));
    let mut world = world_with_a_and_b(4);
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(4), 1);
    assert_eq!(
        saw_both.load(Ordering::SeqCst),
        2,
        "both processors were running at once"
    );
}

#[test]
fn declared_order_constraints_are_respected() {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut schedule = Schedule::new();
    let log = |name: &'static str| {
        let order = order.clone();
        move |_: &mut View<'_>, _: &SystemCtx<'_>| order.lock().unwrap().push(name)
    };
    // Registered first, but must run after "b"; "b" itself must run before "c".
    schedule.add_processor(
        processor("a", Phase::PostPhysics)
            .writes::<A>()
            .after("b")
            .run(log("a")),
    );
    schedule.add_processor(
        processor("c", Phase::PostPhysics)
            .writes::<Pos>()
            .run(log("c")),
    );
    schedule.add_processor(
        processor("b", Phase::PostPhysics)
            .writes::<B>()
            .before("c")
            .run(log("b")),
    );
    let mut world = world_with_a_and_b(1);
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    let order = order.lock().unwrap().clone();
    let at = |n: &str| order.iter().position(|x| *x == n).unwrap();
    assert!(at("b") < at("a") && at("b") < at("c"), "{order:?}");
}

#[test]
fn schedule_cycle_is_an_error_naming_the_cycle() {
    let mut schedule = Schedule::new();
    let noop = |_: &mut View<'_>, _: &SystemCtx<'_>| {};
    schedule.add_processor(
        processor("left", Phase::PostPhysics)
            .after("right")
            .run(noop),
    );
    schedule.add_processor(
        processor("right", Phase::PostPhysics)
            .after("left")
            .run(noop),
    );
    let mut world = World::new();
    let err = SimLoop::new(TickRate::HZ_60, schedule, &mut world)
        .err()
        .unwrap();
    match &err {
        ScheduleError::Cycle { processors } => {
            assert!(
                processors.contains(&"left".to_owned()) && processors.contains(&"right".to_owned())
            );
        }
        other => panic!("expected a cycle, got {other}"),
    }
    assert!(err.to_string().contains("left") && err.to_string().contains("right"));
}

#[test]
fn unknown_order_target_and_duplicate_names_are_errors() {
    let noop = |_: &mut View<'_>, _: &SystemCtx<'_>| {};
    let mut schedule = Schedule::new();
    schedule.add_processor(processor("a", Phase::PostPhysics).after("ghost").run(noop));
    let err = SimLoop::new(TickRate::HZ_60, schedule, &mut World::new())
        .err()
        .unwrap();
    assert!(
        matches!(err, ScheduleError::UnknownProcessor { .. }),
        "{err}"
    );
    assert!(err.to_string().contains("ghost"));

    let mut schedule = Schedule::new();
    schedule.add_processor(processor("a", Phase::PostPhysics).run(noop));
    schedule.add_processor(processor("a", Phase::PrePhysics).run(noop));
    let err = SimLoop::new(TickRate::HZ_60, schedule, &mut World::new())
        .err()
        .unwrap();
    assert!(matches!(err, ScheduleError::DuplicateName { .. }), "{err}");
}

#[test]
fn phases_run_in_order_whatever_the_registration_order() {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut schedule = Schedule::new();
    let phases = [
        Phase::Presentation,
        Phase::FrameEnd,
        Phase::PostPhysics,
        Phase::Physics,
        Phase::PrePhysics,
    ];
    for phase in phases {
        let order = order.clone();
        schedule.add_processor(
            processor(&format!("{phase:?}"), phase)
                .run(move |_: &mut View<'_>, _: &SystemCtx<'_>| order.lock().unwrap().push(phase)),
        );
    }
    let mut world = World::new();
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(2), 1);
    assert_eq!(
        *order.lock().unwrap(),
        [
            Phase::PrePhysics,
            Phase::Physics,
            Phase::PostPhysics,
            Phase::FrameEnd,
            Phase::Presentation
        ]
    );
    assert_eq!(Phase::ALL.len(), 5);
}

#[test]
fn undeclared_access_is_an_error_not_a_race() {
    let seen = Arc::new(std::sync::Mutex::new(None));
    let mut schedule = Schedule::new();
    let out = seen.clone();
    schedule.add_processor(processor("sneaky", Phase::PostPhysics).reads::<A>().run(
        move |view: &mut View<'_>, _: &SystemCtx<'_>| {
            let read_b = view.query::<&B>().err().map(|e| e.to_string());
            let write_a = view.query::<&mut A>().err().map(|e| e.to_string());
            *out.lock().unwrap() = Some((read_b, write_a));
        },
    ));
    let mut world = world_with_a_and_b(2);
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(2), 1);
    let (read_b, write_a) = seen.lock().unwrap().clone().unwrap();
    assert!(
        read_b.unwrap().contains("B"),
        "reading an undeclared component is refused"
    );
    assert!(
        write_a.unwrap().contains("A"),
        "writing a read-only component is refused"
    );
}

/// The schedule of the determinism test, as parallel processors.
fn parallel_schedule() -> Schedule {
    let mut schedule = Schedule::new();
    schedule.add_processor(
        processor("integrate", Phase::PostPhysics)
            .writes::<Pos>()
            .reads::<Vel>()
            .run(|view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                let dt = ctx.dt;
                view.query::<(&mut Pos, &Vel)>().unwrap().par_for_each(
                    ctx.jobs,
                    CHUNK,
                    |_, (p, v)| {
                        p.x += v.x * dt;
                        p.y += v.y * dt;
                    },
                );
            }),
    );
    schedule.add_processor(processor("count", Phase::PostPhysics).writes::<A>().run(
        |view: &mut View<'_>, _: &SystemCtx<'_>| {
            view.query::<&mut A>()
                .unwrap()
                .for_each(|_, a| a.0 = a.0.wrapping_mul(31).wrapping_add(7));
        },
    ));
    schedule.add_processor(
        processor("drift", Phase::PostPhysics)
            .writes::<B>()
            .reads::<Pos>()
            .run(|view: &mut View<'_>, _: &SystemCtx<'_>| {
                view.query::<(&mut B, &Pos)>()
                    .unwrap()
                    .for_each(|_, (b, p)| b.0 += p.x * 0.001);
            }),
    );
    schedule
}

fn run_parallel(threads: usize, ticks: u32) -> WorldHash {
    let mut world = world_with_a_and_b(3_000);
    let mut sim = SimLoop::new(TickRate::HZ_60, parallel_schedule(), &mut world).unwrap();
    sim.step(&mut world, &jobs(threads), ticks);
    world.state_hash().unwrap()
}

#[test]
fn world_hash_identical_for_1_and_8_threads_with_parallel_processors() {
    let expected = run_parallel(1, 120);
    let mut idle = world_with_a_and_b(3_000);
    SimLoop::new(TickRate::HZ_60, Schedule::new(), &mut idle)
        .unwrap()
        .step(&mut idle, &jobs(1), 120);
    assert_ne!(
        expected,
        idle.state_hash().unwrap(),
        "the processors changed the world"
    );
    for threads in [2, 4, 8] {
        assert_eq!(run_parallel(threads, 120), expected, "{threads} threads");
    }
}

/// Presentation-only state (e.g. a representation LOD bucket); never hashed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
struct Glow(f32);

#[test]
fn presentation_phase_never_changes_world_hash() {
    let hash_after = |with_presentation: bool| {
        let mut schedule = parallel_schedule();
        if with_presentation {
            schedule.add_processor(
                processor("glow", Phase::Presentation)
                    .reads::<Pos>()
                    .writes_presentation::<Glow>()
                    .run(|view: &mut View<'_>, _: &SystemCtx<'_>| {
                        view.query::<(&mut Glow, &Pos)>()
                            .unwrap()
                            .for_each(|_, (g, p)| g.0 = p.x);
                    }),
            );
        }
        let mut world = world_with_a_and_b(50);
        // Glow is presentation state whether or not a processor writes it this run.
        world.register_presentation::<Glow>();
        let ids: Vec<_> = world.entity_ids_sorted();
        for id in ids {
            world.insert(id, Glow(0.0)).unwrap();
        }
        let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
        sim.step(&mut world, &jobs(2), 30);
        let mut glow = 0.0;
        world
            .query::<&Glow>()
            .unwrap()
            .for_each(&mut world, |_, g| glow += g.0.abs());
        (world.state_hash().unwrap(), glow)
    };
    let (with, glow) = hash_after(true);
    let (without, none) = hash_after(false);
    assert!(
        glow > 0.0 && none == 0.0,
        "the presentation processor ran and wrote Glow"
    );
    assert_eq!(with, without);
}

#[test]
fn presentation_and_simulation_state_stay_apart() {
    let noop = |_: &mut View<'_>, _: &SystemCtx<'_>| {};
    let mut schedule = Schedule::new();
    schedule.add_processor(
        processor("leak", Phase::Presentation)
            .writes::<Pos>()
            .run(noop),
    );
    let err = SimLoop::new(TickRate::HZ_60, schedule, &mut World::new())
        .err()
        .unwrap();
    assert!(
        matches!(err, ScheduleError::PresentationWritesSimulation { .. }),
        "{err}"
    );

    let mut schedule = Schedule::new();
    schedule.add_processor(
        processor("glow", Phase::Presentation)
            .writes_presentation::<Glow>()
            .run(noop),
    );
    schedule.add_processor(
        processor("peek", Phase::PostPhysics)
            .reads::<Glow>()
            .run(noop),
    );
    let err = SimLoop::new(TickRate::HZ_60, schedule, &mut World::new())
        .err()
        .unwrap();
    assert!(
        matches!(err, ScheduleError::SimulationReadsPresentation { .. }),
        "{err}"
    );
    assert!(err.to_string().contains("peek"));
}

#[test]
fn exclusive_systems_still_run_with_the_whole_world() {
    let mut schedule = Schedule::new();
    schedule.add_in(
        Phase::PrePhysics,
        system("spawn_counter", |world: &mut World, _: &SystemCtx<'_>| {
            let n = world.entity_ids_sorted().len() as u32;
            world.insert_resource(n);
        }),
    );
    // The plain `add` keeps working: an exclusive system in PostPhysics.
    schedule.add(system("double", |world: &mut World, _: &SystemCtx<'_>| {
        if let Some(n) = world.resource_mut::<u32>() {
            *n *= 2;
        }
    }));
    let mut world = world_with_a_and_b(5);
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(4), 1);
    assert_eq!(world.resource::<u32>(), Some(&10));
}

#[test]
fn time_scale_scales_sim_dt_deterministically() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut schedule = Schedule::new();
    let out = seen.clone();
    schedule.add(system(
        "watch_dt",
        move |_: &mut World, ctx: &SystemCtx<'_>| {
            out.lock().unwrap().push(ctx.dt);
        },
    ));
    let mut world = World::new();
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(1), 1);
    let before = world.state_hash().unwrap();
    world.resource_mut::<SimClock>().unwrap().time_scale = TimeScale::new(0.25).unwrap();
    assert_ne!(
        world.state_hash().unwrap(),
        before,
        "the time scale is simulation state"
    );
    sim.step(&mut world, &jobs(1), 1);
    let dts = seen.lock().unwrap().clone();
    assert!(
        (dts[0] - 1.0 / 60.0).abs() < 1e-7 && (dts[1] - 0.25 / 60.0).abs() < 1e-7,
        "{dts:?}"
    );
    let clock = *world.resource::<SimClock>().unwrap();
    assert_eq!(clock.tick.0, 2, "scaled ticks still count as ticks");
    assert!(
        (clock.time - 1.25 / 60.0).abs() < 1e-9,
        "sim time advances by scaled dt: {}",
        clock.time
    );

    for bad in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(TimeScale::new(bad).is_err(), "{bad}");
    }
    assert!(
        TimeScale::new(0.0).is_ok(),
        "zero freezes simulated time (hit-stop)"
    );
}

#[test]
fn view_queries_filter_with_and_without() {
    // Six entities with A; the even ones also get B and the first two a Marker tag. A processor
    // reading all three sees 3 with B, 3 without B, and 2 with the tag.
    let seen = Arc::new(std::sync::Mutex::new(None));
    let mut schedule = Schedule::new();
    let out = seen.clone();
    schedule.add_processor(
        processor("filtered", Phase::PostPhysics)
            .reads::<A>()
            .reads::<B>()
            .reads::<common::Marker>()
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                let mut with_b = Vec::new();
                let mut without_b = Vec::new();
                let mut tagged = Vec::new();
                view.query_filtered::<&A, concordia_terra::With<B>>()
                    .unwrap()
                    .for_each(|_, a| with_b.push(a.0));
                view.query_filtered::<&A, concordia_terra::Without<B>>()
                    .unwrap()
                    .for_each(|_, a| without_b.push(a.0));
                view.query_filtered::<&A, concordia_terra::With<common::Marker>>()
                    .unwrap()
                    .for_each(|_, a| tagged.push(a.0));
                let undeclared = view
                    .query_filtered::<&A, concordia_terra::Without<Pos>>()
                    .err()
                    .map(|e| e.to_string());
                for v in [&mut with_b, &mut without_b, &mut tagged] {
                    v.sort_unstable();
                }
                *out.lock().unwrap() = Some((with_b, without_b, tagged, undeclared));
            }),
    );
    let mut world = World::new();
    for (i, id) in (0u32..).zip(common::ids(6)) {
        world.spawn_with_id(id).unwrap();
        world.insert(id, A(i)).unwrap();
        if i % 2 == 0 {
            world.insert(id, B(0.0)).unwrap();
        }
        if i < 2 {
            world.insert(id, common::Marker).unwrap();
        }
    }
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(&mut world, &jobs(2), 1);
    let (with_b, without_b, tagged, undeclared) = seen.lock().unwrap().clone().unwrap();
    assert_eq!(with_b, vec![0, 2, 4]);
    assert_eq!(without_b, vec![1, 3, 5]);
    assert_eq!(tagged, vec![0, 1]);
    assert!(
        undeclared.is_some_and(|e| e.contains("Pos")),
        "a filter on an undeclared component is refused"
    );
}
