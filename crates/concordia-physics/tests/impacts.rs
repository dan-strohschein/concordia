//! Contact impacts (M8 8.0b): entities with `ContactReports` hear how hard a contact began
//! (`ContactImpact`: closing speed along the normal times the pair's reduced mass, in N·s) and
//! where. Others hear nothing new, so existing scenes raise exactly the signals they did.
//!
//! A 2 kg ball dropped 5 m onto the static ground closes at √(2 · 9.81 · 5) ≈ 9.9 m/s; with the
//! ground's infinite mass the reduced mass is 2 kg, so the impact is ≈ 19.8 N·s (Jolt detects the
//! contact a little before touching, within a tick's fall: ±15 %), at the ball's foot (y ≈ 0).

mod common;

use std::sync::{Arc, Mutex};

use common::Sim;
use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;
use concordia_core::math::Vec3;
use concordia_core::time::TickRate;
use concordia_physics::{Collider, ColliderShape, ContactImpact, ContactReports, RigidBody};
use concordia_terra::{Phase, Schedule, SimLoop, SystemCtx, View, World, processor, system};

type Log = Arc<Mutex<Vec<(EntityId, ContactImpact)>>>;

fn run(sim: Sim, ticks: u32) -> Vec<(EntityId, ContactImpact)> {
    let log = Log::default();
    let Sim {
        mut world,
        mut physics,
        ..
    } = sim;
    let mut schedule = Schedule::new();
    schedule.add_in(
        Phase::Physics,
        system(
            "physics.step",
            move |world: &mut World, ctx: &SystemCtx<'_>| {
                physics.tick(world, ctx.dt).unwrap();
            },
        ),
    );
    let sink = Arc::clone(&log);
    schedule.add_processor(
        processor("test.impacts", Phase::PostPhysics)
            .receives::<ContactImpact>()
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                let mut log = sink.lock().unwrap();
                for (id, c) in view.signals::<ContactImpact>().unwrap() {
                    log.push((*id, c.clone()));
                }
            }),
    );
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    sim.step(
        &mut world,
        &JobSystem::new(std::num::NonZeroUsize::MIN).unwrap(),
        ticks,
    );
    log.lock().unwrap().clone()
}

fn ball(sim: &mut Sim, x: f32) -> EntityId {
    let body = RigidBody {
        mass: Some(2.0),
        restitution: 0.0,
        ..RigidBody::dynamic()
    };
    sim.spawn(
        Vec3::new(x, 5.5, 0.0),
        body,
        Collider::new(ColliderShape::Sphere { radius: 0.5 }),
    )
}

#[test]
fn a_reporting_ball_hears_how_hard_it_hit_the_ground() {
    let mut sim = Sim::new();
    let ground = sim.ground();
    let heard = ball(&mut sim, 0.0);
    sim.world
        .insert(heard, ContactReports { min_strength: 1.0 })
        .unwrap();
    let deaf = ball(&mut sim, 3.0);
    let picky = ball(&mut sim, -3.0);
    sim.world
        .insert(
            picky,
            ContactReports {
                min_strength: 1000.0,
            },
        )
        .unwrap();
    let log = run(sim, 90);
    assert_eq!(log.len(), 1, "only the reporting ball, once: {log:?}");
    let (on, impact) = &log[0];
    assert_eq!(*on, heard);
    assert_eq!(impact.other, ground);
    let expected = 2.0 * (2.0 * 9.81 * 5.0f32).sqrt();
    assert!(
        (impact.strength - expected).abs() < 0.15 * expected,
        "{impact:?}, expected ≈ {expected}"
    );
    assert!(
        impact.point.y.abs() < 0.1 && impact.point.x.abs() < 0.1,
        "{impact:?}"
    );
    assert!(!log.iter().any(|(id, _)| *id == deaf || *id == picky));
}

#[test]
fn contact_reports_reject_bad_thresholds() {
    for bad in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(ContactReports::new(bad).is_err(), "{bad}");
    }
    assert_eq!(ContactReports::new(0.0).unwrap().min_strength, 0.0);
}
