//! Character controller (Jolt CharacterVirtual) (M1 phase 1.7b).

mod common;

use common::Sim;
use concordia_core::math::{Quat, Vec3};
use concordia_physics::{CharacterController, CharacterMotion, Collider, ColliderShape, RigidBody};
use concordia_scene::builtin::Transform;

fn character(sim: &mut Sim, at: Vec3) -> concordia_core::id::EntityId {
    let id = sim.ball(Vec3::ZERO, 0.1); // reuse spawn plumbing, then replace with a character
    sim.world.remove::<RigidBody>(id).unwrap();
    sim.world.remove::<Collider>(id).unwrap();
    sim.world.get_mut::<Transform>(id).unwrap().translation = at;
    sim.world
        .insert(id, CharacterController::default())
        .unwrap();
    sim.world
        .insert(
            id,
            CharacterMotion {
                velocity: Vec3::ZERO,
            },
        )
        .unwrap();
    id
}

fn walk(sim: &mut Sim, id: concordia_core::id::EntityId, velocity: Vec3, ticks: u32) {
    sim.world.get_mut::<CharacterMotion>(id).unwrap().velocity = velocity;
    sim.step(ticks);
}

fn block(sim: &mut Sim, center: Vec3, half: Vec3, rotation: Quat) {
    let id = sim.spawn(
        center,
        RigidBody::fixed(),
        Collider::new(ColliderShape::Box { half_extents: half }),
    );
    sim.world.get_mut::<Transform>(id).unwrap().rotation = rotation;
}

#[test]
fn character_controller_climbs_step_below_step_height_and_not_above() {
    let mut sim = Sim::new();
    sim.ground();
    let step_height = CharacterController::default().step_height;
    block(
        &mut sim,
        Vec3::new(3.0, 0.5 * step_height * 0.8, 0.0),
        Vec3::new(1.0, 0.5 * step_height * 0.8, 3.0),
        Quat::IDENTITY,
    );
    block(
        &mut sim,
        Vec3::new(3.0, step_height * 1.5, 8.0),
        Vec3::new(1.0, step_height * 1.5, 3.0),
        Quat::IDENTITY,
    );
    let low = character(&mut sim, Vec3::new(0.0, 0.0, 0.0));
    let high = character(&mut sim, Vec3::new(0.0, 0.0, 8.0));
    sim.step(30);
    sim.world.get_mut::<CharacterMotion>(high).unwrap().velocity = Vec3::new(2.0, 0.0, 0.0);
    walk(&mut sim, low, Vec3::new(2.0, 0.0, 0.0), 90);
    assert!(
        sim.pos(low).x > 2.5 && sim.pos(low).y > 0.5 * step_height,
        "low step climbed: {}",
        sim.pos(low)
    );
    assert!(sim.pos(high).x < 2.0, "high step blocks: {}", sim.pos(high));
}

#[test]
fn character_controller_slides_along_wall() {
    let mut sim = Sim::new();
    sim.ground();
    block(
        &mut sim,
        Vec3::new(2.0, 1.0, 0.0),
        Vec3::new(0.5, 1.0, 10.0),
        Quat::IDENTITY,
    );
    let id = character(&mut sim, Vec3::ZERO);
    sim.step(30);
    walk(
        &mut sim,
        id,
        Vec3::new(2.0, 0.0, 2.0).normalize() * 3.0,
        120,
    );
    let at = sim.pos(id);
    assert!(at.x < 1.5, "the wall stops x: {at}");
    assert!(at.z > 3.0, "it slides along z: {at}");
}

#[test]
fn character_controller_does_not_walk_up_slope_steeper_than_max() {
    let mut sim = Sim::new();
    sim.ground();
    let max = CharacterController::default().max_slope_deg;
    let steep = (max + 15.0).to_radians();
    block(
        &mut sim,
        Vec3::new(4.0, 0.0, 0.0),
        Vec3::new(3.0, 0.2, 3.0),
        Quat::from_rotation_z(steep),
    );
    let id = character(&mut sim, Vec3::ZERO);
    sim.step(30);
    walk(&mut sim, id, Vec3::new(2.0, 0.0, 0.0), 180);
    assert!(
        sim.pos(id).y < 0.6,
        "stays at the foot of the slope: {}",
        sim.pos(id)
    );
}
