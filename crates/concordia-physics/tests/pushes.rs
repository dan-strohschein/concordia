//! Acting on live bodies (M8 8.4b): a one-shot `Impulse`, a `Sleep` request and
//! `PhysicsDisabled`, which fields use for force, impulse, sleep and disable. Velocity is read
//! only when a body is made, so these are the way to push a body that already exists.

mod common;

use common::Sim;
use concordia_core::math::Vec3;
use concordia_physics::{Impulse, PhysicsDisabled, PhysicsSettings, RigidBody, Sleep};
use concordia_scene::builtin::Velocity;

fn still_sim() -> Sim {
    let mut settings = PhysicsSettings::default();
    settings.gravity = Vec3::ZERO;
    Sim::with(settings, 1)
}

fn ball(sim: &mut Sim) -> concordia_core::id::EntityId {
    let id = sim.ball(Vec3::new(0.0, 5.0, 0.0), 0.5);
    let mut body = sim.world.get::<RigidBody>(id).unwrap().clone();
    body.mass = Some(2.0);
    body.linear_damping = 0.0;
    body.angular_damping = 0.0;
    sim.world.insert(id, body).unwrap();
    id
}

#[test]
fn an_impulse_changes_velocity_once_and_is_consumed() {
    let mut sim = still_sim();
    let id = ball(&mut sim);
    sim.step(2);
    sim.world
        .insert(
            id,
            Impulse {
                linear: Vec3::new(4.0, 0.0, 0.0),
                angular: Vec3::ZERO,
            },
        )
        .unwrap();
    sim.step(1);
    assert!(sim.world.get::<Impulse>(id).is_none(), "consumed");
    let v = sim.world.get::<Velocity>(id).unwrap().linear;
    assert!(
        (v - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-4,
        "4 N·s on 2 kg: {v}"
    );
    sim.step(10);
    let v = sim.world.get::<Velocity>(id).unwrap().linear;
    assert!((v.x - 2.0).abs() < 1e-4, "only once: {v}");
}

#[test]
fn an_angular_impulse_spins_a_body() {
    let mut sim = still_sim();
    let id = ball(&mut sim);
    sim.step(1);
    sim.world
        .insert(
            id,
            Impulse {
                linear: Vec3::ZERO,
                angular: Vec3::new(0.0, 1.0, 0.0),
            },
        )
        .unwrap();
    sim.step(1);
    let w = sim.world.get::<Velocity>(id).unwrap().angular;
    assert!(w.y > 0.1 && w.x.abs() < 1e-4 && w.z.abs() < 1e-4, "{w}");
}

#[test]
fn a_sleep_request_stops_a_moving_body() {
    let mut sim = still_sim();
    let id = ball(&mut sim);
    sim.world
        .insert(
            id,
            Impulse {
                linear: Vec3::new(2.0, 0.0, 0.0),
                angular: Vec3::ZERO,
            },
        )
        .unwrap();
    sim.step(5);
    assert!(sim.speed(id) > 0.5);
    sim.world.insert(id, Sleep {}).unwrap();
    sim.step(1);
    assert!(sim.world.get::<Sleep>(id).is_none(), "consumed");
    let at = sim.pos(id);
    sim.step(30);
    assert_eq!(sim.speed(id), 0.0, "asleep");
    assert_eq!(sim.pos(id), at, "and still");
}

#[test]
fn disabled_bodies_leave_the_world_and_come_back() {
    let mut sim = Sim::new();
    let id = ball(&mut sim);
    sim.step(1);
    let bodies = sim.physics.body_count();
    sim.world.insert(id, PhysicsDisabled {}).unwrap();
    sim.step(1);
    assert_eq!(
        sim.physics.body_count(),
        bodies - 1,
        "out of the Jolt world"
    );
    let at = sim.pos(id);
    sim.step(30);
    assert_eq!(sim.pos(id), at, "nothing moves it while disabled");
    sim.world.remove::<PhysicsDisabled>(id).unwrap();
    sim.step(30);
    assert_eq!(sim.physics.body_count(), bodies);
    assert!(sim.pos(id).y < at.y, "back, and falling");
}
