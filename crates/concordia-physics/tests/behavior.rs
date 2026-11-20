//! Rigid-body behaviour through the ECS sync (M1 phase 1.7b).

mod common;

use common::{DT, Sim, body};
use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;
use concordia_core::math::Vec3;
use concordia_core::time::TickRate;
use concordia_physics::{
    BodyKind, Collider, ColliderShape, ContactBegan, ContactEnded, PhysicsLayer, PhysicsSettings,
    RigidBody,
};
use concordia_scene::builtin::{Transform, Velocity};
use concordia_terra::{Phase, Schedule, SimLoop, SystemCtx, View, World, processor, system};

#[test]
fn sphere_falls_under_gravity_matches_analytic_within_tolerance() {
    let mut sim = Sim::new();
    let mut free = RigidBody::dynamic();
    free.linear_damping = 0.0;
    let ball = sim.spawn(
        Vec3::new(0.0, 100.0, 0.0),
        free,
        Collider::new(ColliderShape::Sphere { radius: 0.5 }),
    );
    sim.step(60);
    let fallen = 100.0 - sim.pos(ball).y;
    // Jolt integrates semi-implicitly: after n steps the fall is g·dt²·n(n+1)/2, which is
    // 1/n above the continuous ½gt² (1.7% at 60 Hz).
    let discrete = 9.81 * common::DT * common::DT * (60.0 * 61.0) / 2.0;
    let analytic = 0.5 * 9.81 * 1.0;
    assert!(
        (fallen - discrete).abs() / discrete < 0.001,
        "fell {fallen}, discrete {discrete}"
    );
    assert!(
        (fallen - analytic).abs() / analytic < 0.02,
        "fell {fallen}, analytic {analytic}"
    );
    assert!(sim.world.get::<Velocity>(ball).unwrap().linear.y < -9.0);
}

#[test]
fn initial_velocity_is_not_integrated_twice() {
    let mut settings = PhysicsSettings::default();
    settings.gravity = Vec3::ZERO;
    let mut sim = Sim::with(settings, 1);
    let mut free = RigidBody::dynamic();
    free.linear_damping = 0.0;
    let ball = sim.spawn(
        Vec3::ZERO,
        free,
        Collider::new(ColliderShape::Sphere { radius: 0.5 }),
    );
    sim.world
        .insert(
            ball,
            Velocity {
                linear: Vec3::new(3.0, 0.0, 0.0),
                angular: Vec3::ZERO,
            },
        )
        .unwrap();
    sim.step(60);
    assert!((sim.pos(ball).x - 3.0).abs() < 0.01, "{}", sim.pos(ball));
}

#[test]
fn box_rests_on_static_plane() {
    let mut sim = Sim::new();
    sim.ground();
    let cube = sim.cube(Vec3::new(0.0, 2.0, 0.0));
    sim.step(180);
    assert!(sim.speed(cube) < 1e-3, "speed {}", sim.speed(cube));
    assert!((sim.pos(cube).y - 0.5).abs() < 0.02, "{}", sim.pos(cube));
}

#[test]
fn box_stack_of_10_is_stable_for_10_seconds() {
    let mut sim = Sim::new();
    sim.ground();
    // Authored with a 1 cm gap, as an editor would place them; contact slop lets them sink ~1 mm each.
    let top = (0..10)
        .map(|i| sim.cube(Vec3::new(0.0, 0.5 + i as f32 * 1.01, 0.0)))
        .last()
        .unwrap();
    sim.step(60);
    let settled = sim.pos(top);
    sim.step(540);
    let creep = sim.pos(top).distance(settled);
    assert!(creep < 0.001, "top box crept {creep} m after settling");
    assert!(
        sim.pos(top).distance(Vec3::new(0.0, 9.5, 0.0)) < 0.02,
        "{}",
        sim.pos(top)
    );
}

#[test]
fn restitution_one_sphere_bounces_to_near_original_height() {
    let mut sim = Sim::new();
    let mut floor = RigidBody::fixed();
    floor.restitution = 1.0;
    sim.spawn(
        Vec3::new(0.0, -0.5, 0.0),
        floor,
        Collider::new(ColliderShape::Box {
            half_extents: Vec3::new(10.0, 0.5, 10.0),
        }),
    );
    let mut bouncy = RigidBody::dynamic();
    bouncy.restitution = 1.0;
    bouncy.linear_damping = 0.0;
    let ball = sim.spawn(
        Vec3::new(0.0, 5.0, 0.0),
        bouncy,
        Collider::new(ColliderShape::Sphere { radius: 0.5 }),
    );
    let mut peak_after_bounce = 0.0_f32;
    let mut bounced = false;
    for _ in 0..240 {
        sim.step(1);
        let v = sim.world.get::<Velocity>(ball).unwrap().linear.y;
        bounced |= v > 0.5;
        if bounced {
            peak_after_bounce = peak_after_bounce.max(sim.pos(ball).y);
        }
    }
    assert!(bounced);
    assert!(
        peak_after_bounce > 4.5,
        "bounced back to {peak_after_bounce}"
    );
}

/// One delivered contact signal: (tick, receiver, began?, other, sensor).
type ContactLog = std::sync::Arc<std::sync::Mutex<Vec<(u64, EntityId, bool, EntityId, bool)>>>;

/// Steps physics in its phase and logs contact signals as a `PostPhysics` processor receives them.
fn contact_loop(sim: Sim, log: ContactLog) -> (World, SimLoop) {
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
    schedule.add_processor(
        processor("contacts", Phase::PostPhysics)
            .receives::<ContactBegan>()
            .receives::<ContactEnded>()
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                let mut log = log.lock().unwrap();
                for (id, c) in view.signals::<ContactBegan>().unwrap() {
                    log.push((ctx.tick.0, *id, true, c.other, c.sensor));
                }
                for (id, c) in view.signals::<ContactEnded>().unwrap() {
                    log.push((ctx.tick.0, *id, false, c.other, c.sensor));
                }
            }),
    );
    let sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    (world, sim)
}

#[test]
fn sensor_enter_and_exit_arrive_as_signals() {
    let mut sim = Sim::new();
    let mut zone = Collider::new(ColliderShape::Box {
        half_extents: Vec3::splat(1.0),
    });
    zone.is_sensor = true;
    let sensor = sim.spawn(Vec3::new(0.0, 5.0, 0.0), RigidBody::fixed(), zone);
    let ball = sim.ball(Vec3::new(0.0, 10.0, 0.0), 0.25);
    let log = ContactLog::default();
    let (mut world, mut sim) = contact_loop(sim, log.clone());
    let jobs = JobSystem::new(std::num::NonZeroUsize::MIN).unwrap();
    sim.step(&mut world, &jobs, 120);
    let log = log.lock().unwrap().clone();
    let for_sensor: Vec<_> = log.iter().filter(|e| e.1 == sensor).collect();
    let for_ball: Vec<_> = log.iter().filter(|e| e.1 == ball).collect();
    // Entered once and exited once, each delivered to both entities, naming the other one.
    assert_eq!(
        for_sensor
            .iter()
            .map(|e| (e.2, e.3, e.4))
            .collect::<Vec<_>>(),
        [(true, ball, true), (false, ball, true)],
        "{log:?}"
    );
    assert_eq!(
        for_ball.iter().map(|e| (e.2, e.3, e.4)).collect::<Vec<_>>(),
        [(true, sensor, true), (false, sensor, true)],
        "{log:?}"
    );
    assert!(
        for_sensor[0].0 > 0,
        "raised by the physics step, delivered on a later tick"
    );
    assert!(for_sensor[0].0 < for_sensor[1].0);
}

#[test]
fn layers_filter_collisions() {
    let mut settings = PhysicsSettings::default();
    settings.set_collides(
        PhysicsLayer::new(2).unwrap(),
        PhysicsLayer::new(0).unwrap(),
        false,
    );
    let mut sim = Sim::with(settings, 1);
    sim.ground();
    let mut ghost = Collider::new(ColliderShape::Sphere { radius: 0.5 });
    ghost.layer = PhysicsLayer::new(2).unwrap();
    let falls_through = sim.spawn(Vec3::new(0.0, 2.0, 0.0), RigidBody::dynamic(), ghost);
    let lands = sim.ball(Vec3::new(3.0, 2.0, 0.0), 0.5);
    sim.step(120);
    assert!(
        sim.pos(falls_through).y < -1.0,
        "{}",
        sim.pos(falls_through)
    );
    assert!(sim.pos(lands).y > 0.4, "{}", sim.pos(lands));
}

#[test]
fn despawning_entity_removes_body() {
    let mut sim = Sim::new();
    sim.ground();
    let cube = sim.cube(Vec3::new(0.0, 0.5, 0.0));
    sim.step(1);
    assert_eq!(sim.physics.body_count(), 2);
    sim.world.despawn(cube).unwrap();
    sim.step(1);
    assert_eq!(sim.physics.body_count(), 1);
    let ball = sim.ball(Vec3::new(0.0, 3.0, 0.0), 0.5);
    sim.step(120);
    assert!(
        sim.pos(ball).y < 0.6,
        "the removed cube no longer blocks: {}",
        sim.pos(ball)
    );
}

#[test]
fn kinematic_body_follows_its_transform_and_pushes_dynamics() {
    let mut sim = Sim::new();
    sim.ground();
    let pusher = sim.spawn(
        Vec3::new(-3.0, 0.5, 0.0),
        body(BodyKind::Kinematic),
        Collider::new(ColliderShape::Box {
            half_extents: Vec3::splat(0.5),
        }),
    );
    let cube = sim.cube(Vec3::new(0.0, 0.5, 0.0));
    for i in 0..120 {
        let x = -3.0 + 4.0 * (i as f32 / 120.0);
        sim.world
            .get_mut::<Transform>(pusher)
            .unwrap()
            .translation
            .x = x;
        sim.step(1);
    }
    assert!(
        (sim.pos(pusher).x - (1.0 - 4.0 / 120.0)).abs() < 0.05,
        "{}",
        sim.pos(pusher)
    );
    assert!(
        sim.pos(cube).x > 1.2,
        "the cube was pushed: {}",
        sim.pos(cube)
    );
}

#[test]
fn invalid_collider_is_reported_and_skipped_not_panicked() {
    let mut sim = Sim::new();
    let bad = sim.spawn(
        Vec3::ZERO,
        RigidBody::dynamic(),
        Collider::new(ColliderShape::Sphere { radius: -1.0 }),
    );
    let good = sim.ball(Vec3::new(3.0, 5.0, 0.0), 0.5);
    let report = sim.physics.tick(&mut sim.world, DT).unwrap();
    assert_eq!(report.rejected.len(), 1, "{report:?}");
    assert_eq!(report.rejected[0].0, bad);
    assert!(report.rejected[0].1.contains("radius"), "{report:?}");
    assert_eq!(sim.physics.body_count(), 1);
    assert!(sim.pos(good).y < 5.0, "valid bodies still step");
}
