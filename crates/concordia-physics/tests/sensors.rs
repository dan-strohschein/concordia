//! Sensors see moving kinematic bodies and characters (M2 phase 2.10a, gap S2).
//!
//! Every mover starts at x = −5 and moves +3 m/s along x (0.05 m per tick, applied in
//! `PrePhysics` before the tick's step, so after tick t it is at −5 + 0.05 (t + 1)). The sensor
//! is a 1 m half-extent box at the origin. A sphere of radius 0.25 first overlaps it once
//! x > −1.25 (tick 75) and leaves once x > 1.25 (tick 125); a character's capsule (radius 0.3)
//! once x > −1.3 (tick 74) and x > 1.3 (tick 126). Signals arrive on the next tick, and the
//! broad phase may pad by a few centimetres, so each window allows ±3 ticks. Sensors float
//! 0.1 m above the floor so a kinematic one never touches it.

mod common;

use std::sync::{Arc, Mutex};

use common::{Sim, body};
use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;
use concordia_core::math::Vec3;
use concordia_core::time::TickRate;
use concordia_physics::{
    BodyKind, CharacterController, CharacterMotion, Collider, ColliderShape, ContactBegan,
    ContactEnded,
};
use concordia_scene::builtin::Transform;
use concordia_terra::{Phase, Schedule, SimLoop, SystemCtx, View, World, processor, system};

/// One delivered contact signal: (tick, receiver, began?, other).
type Log = Arc<Mutex<Vec<(u64, EntityId, bool, EntityId)>>>;

const SPEED: f32 = 3.0;

/// Moves `movers` +x at 3 m/s in `PrePhysics`, steps physics, and logs contact signals.
fn run(sim: Sim, movers: Vec<EntityId>, ticks: u32) -> Vec<(u64, EntityId, bool, EntityId)> {
    let log = Log::default();
    let Sim {
        mut world,
        mut physics,
        ..
    } = sim;
    let mut schedule = Schedule::new();
    schedule.add_in(
        Phase::PrePhysics,
        system(
            "test.move",
            move |world: &mut World, ctx: &SystemCtx<'_>| {
                for id in &movers {
                    if let Some(t) = world.get_mut::<Transform>(*id) {
                        t.translation.x += SPEED * ctx.dt;
                    }
                }
            },
        ),
    );
    schedule.add_in(
        Phase::Physics,
        system(
            "physics.step",
            move |world: &mut World, ctx: &SystemCtx<'_>| {
                let report = physics.tick(world, ctx.dt).unwrap();
                assert!(report.rejected.is_empty(), "{report:?}");
            },
        ),
    );
    let sink = Arc::clone(&log);
    schedule.add_processor(
        processor("test.contacts", Phase::PostPhysics)
            .receives::<ContactBegan>()
            .receives::<ContactEnded>()
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                let mut log = sink.lock().unwrap();
                for (id, c) in view.signals::<ContactBegan>().unwrap() {
                    log.push((ctx.tick.0, *id, true, c.other));
                }
                for (id, c) in view.signals::<ContactEnded>().unwrap() {
                    log.push((ctx.tick.0, *id, false, c.other));
                }
            }),
    );
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    let jobs = JobSystem::new(std::num::NonZeroUsize::MIN).unwrap();
    sim.step(&mut world, &jobs, ticks);
    log.lock().unwrap().clone()
}

fn sensor(sim: &mut Sim, kind: BodyKind) -> EntityId {
    let mut zone = Collider::new(ColliderShape::Box {
        half_extents: Vec3::splat(1.0),
    });
    zone.is_sensor = true;
    sim.spawn(Vec3::new(0.0, 1.1, 0.0), body(kind), zone)
}

fn kinematic_ball(sim: &mut Sim, x: f32) -> EntityId {
    sim.spawn(
        Vec3::new(x, 1.1, 0.0),
        body(BodyKind::Kinematic),
        Collider::new(ColliderShape::Sphere { radius: 0.25 }),
    )
}

fn character(sim: &mut Sim) -> EntityId {
    let id = kinematic_ball(sim, -5.0);
    sim.world
        .remove::<concordia_physics::RigidBody>(id)
        .unwrap();
    sim.world.remove::<Collider>(id).unwrap();
    sim.world.get_mut::<Transform>(id).unwrap().translation = Vec3::new(-5.0, 0.0, 0.0);
    sim.world
        .insert(id, CharacterController::default())
        .unwrap();
    sim.world
        .insert(
            id,
            CharacterMotion {
                velocity: Vec3::new(SPEED, 0.0, 0.0),
            },
        )
        .unwrap();
    id
}

/// Asserts `receiver` heard exactly one enter from `other` near `enter` and one exit near `exit`.
fn heard_once(
    log: &[(u64, EntityId, bool, EntityId)],
    receiver: EntityId,
    other: EntityId,
    enter: u64,
    exit: u64,
) {
    let mine: Vec<_> = log
        .iter()
        .filter(|e| e.1 == receiver)
        .map(|e| (e.0, e.2, e.3))
        .collect();
    assert_eq!(mine.len(), 2, "one enter and one exit: {log:?}");
    let near = |tick: u64, want: u64| tick.abs_diff(want) <= 3;
    assert!(
        mine[0].1 && mine[0].2 == other && near(mine[0].0, enter),
        "{mine:?}"
    );
    assert!(
        !mine[1].1 && mine[1].2 == other && near(mine[1].0, exit),
        "{mine:?}"
    );
}

#[test]
fn static_sensor_reports_a_kinematic_body_following_its_transform() {
    let mut sim = Sim::new();
    let zone = sensor(&mut sim, BodyKind::Static);
    let ball = kinematic_ball(&mut sim, -5.0);
    let log = run(sim, vec![ball], 200);
    // Overlap from tick 75, delivered on 76; apart from tick 125, delivered on 126.
    heard_once(&log, zone, ball, 76, 126);
    heard_once(&log, ball, zone, 76, 126);
}

#[test]
fn kinematic_sensor_reports_a_kinematic_body() {
    // A still kinematic sensor and a moving kinematic body (a standing townsperson's fear
    // radius and a wisp), then a moving sensor and a still body: the same windows.
    let mut sim = Sim::new();
    let zone = sensor(&mut sim, BodyKind::Kinematic);
    let ball = kinematic_ball(&mut sim, -5.0);
    let log = run(sim, vec![ball], 200);
    heard_once(&log, zone, ball, 76, 126);

    let mut sim = Sim::new();
    let zone = sensor(&mut sim, BodyKind::Kinematic);
    sim.world.get_mut::<Transform>(zone).unwrap().translation.x = -5.0;
    let ball = kinematic_ball(&mut sim, 0.0);
    let log = run(sim, vec![zone], 200);
    heard_once(&log, ball, zone, 76, 126);
}

#[test]
fn sensors_report_a_character_controller() {
    // A static sensor (a lamp's light) and a kinematic one (a wagon) both hear the walking
    // character, and the character hears them; a capsule of radius 0.3 enters at tick 74.
    let mut sim = Sim::new();
    sim.ground();
    let lamp = sensor(&mut sim, BodyKind::Static);
    let wagon = sensor(&mut sim, BodyKind::Kinematic);
    sim.world.get_mut::<Transform>(wagon).unwrap().translation.z = 10.0;
    let walker = character(&mut sim);
    let log = run(sim, vec![], 200);
    heard_once(&log, lamp, walker, 75, 127);
    heard_once(&log, walker, lamp, 75, 127);
    assert!(
        log.iter().all(|e| e.1 != wagon),
        "the wagon is 10 m away: {log:?}"
    );
}

#[test]
fn character_never_collides_with_its_own_inner_body() {
    // On open ground the character walks 60 ticks × 0.05 m = 3 m (from −5 to −2) and stays on
    // the floor, exactly as without an inner body; a guard for the S2 fix (passes before it).
    let mut sim = Sim::new();
    sim.ground();
    let walker = character(&mut sim);
    sim.step(60);
    let at = sim.pos(walker);
    assert!((at.x - (-2.0)).abs() < 0.02, "x = {}", at.x);
    assert!(at.y.abs() < 0.02, "y = {}", at.y);
}

#[test]
fn inner_body_reports_no_contacts_with_solid_bodies() {
    // The character walks from x = −5 through a 1 m dynamic cube resting at x = 0 (it pushes it,
    // as before 2.10a), and over the floor: its inner body is for sensors only, so no contact
    // signal ever names the character. Written after the S2b fix (a guard for the validation).
    let mut sim = Sim::new();
    sim.ground();
    sim.cube(Vec3::new(0.0, 0.5, 0.0));
    let walker = character(&mut sim);
    let log = run(sim, vec![], 200);
    assert!(
        log.iter().all(|e| e.1 != walker && e.3 != walker),
        "{log:?}"
    );
}

#[test]
fn characters_still_pass_through_each_other() {
    // Jolt characters never collided with each other (the crowd keeps them apart), and inner
    // bodies must not change that: two characters on one line walk head-on at 3 m/s from x = ∓2;
    // after 80 ticks each has moved 80 × 0.05 = 4 m, so they have swapped places.
    let mut sim = Sim::new();
    sim.ground();
    let a = character(&mut sim);
    sim.world.get_mut::<Transform>(a).unwrap().translation.x = -2.0;
    let b = character(&mut sim);
    sim.world.get_mut::<Transform>(b).unwrap().translation.x = 2.0;
    sim.world.get_mut::<CharacterMotion>(b).unwrap().velocity = Vec3::new(-SPEED, 0.0, 0.0);
    sim.step(80);
    assert!((sim.pos(a).x - 2.0).abs() < 0.02, "a at {}", sim.pos(a));
    assert!((sim.pos(b).x + 2.0).abs() < 0.02, "b at {}", sim.pos(b));
}
