//! Raycasts and pointer picking (M1 phase 1.7b).

mod common;

use common::Sim;
use concordia_core::math::camera::rh::{proj::directx, view::look_at_mat4};
use concordia_core::math::{Ray, Vec2, Vec3};
use concordia_physics::{Collider, ColliderShape, LayerMask, PhysicsLayer, RigidBody};

#[test]
fn raycast_hits_nearest_body_with_correct_normal_and_entity() {
    let mut sim = Sim::new();
    let sphere = || Collider::new(ColliderShape::Sphere { radius: 1.0 });
    let near = sim.spawn(Vec3::new(0.0, 0.0, -5.0), RigidBody::fixed(), sphere());
    sim.spawn(Vec3::new(0.0, 0.0, -10.0), RigidBody::fixed(), sphere());
    sim.step(1);
    let ray = Ray::new(Vec3::new(0.0, 0.0, 0.0), Vec3::NEG_Z).unwrap();
    let hit = sim
        .physics
        .raycast(&ray, 100.0, LayerMask::ALL)
        .expect("hit");
    assert_eq!(hit.entity, near);
    assert!((hit.distance - 4.0).abs() < 0.05, "{hit:?}");
    assert!(hit.normal.abs_diff_eq(Vec3::Z, 1e-3), "{hit:?}");
    assert!(
        hit.point.abs_diff_eq(Vec3::new(0.0, 0.0, -4.0), 0.05),
        "{hit:?}"
    );
}

#[test]
fn raycast_misses_beyond_max_distance() {
    let mut sim = Sim::new();
    sim.ball(Vec3::new(0.0, 0.0, -50.0), 1.0);
    sim.step(1);
    let ray = Ray::new(Vec3::ZERO, Vec3::NEG_Z).unwrap();
    assert!(sim.physics.raycast(&ray, 10.0, LayerMask::ALL).is_none());
}

#[test]
fn raycast_respects_layer_filter() {
    let mut sim = Sim::new();
    let mut hidden = Collider::new(ColliderShape::Sphere { radius: 1.0 });
    hidden.layer = PhysicsLayer::new(3).unwrap();
    sim.spawn(Vec3::new(0.0, 0.0, -5.0), RigidBody::fixed(), hidden);
    let behind = sim.ball(Vec3::new(0.0, 0.0, -10.0), 1.0);
    sim.step(1);
    let ray = Ray::new(Vec3::ZERO, Vec3::NEG_Z).unwrap();
    let mask = LayerMask::ALL.without(PhysicsLayer::new(3).unwrap());
    assert_eq!(
        sim.physics.raycast(&ray, 100.0, mask).unwrap().entity,
        behind
    );
}

#[test]
fn ortho_pointer_ray_picks_ground_point() {
    let mut sim = Sim::new();
    let ground = sim.ground();
    sim.step(1);
    let viewport = Vec2::new(1280.0, 720.0);
    let target = Vec3::new(2.0, 0.0, -3.0);
    let view = look_at_mat4(target + Vec3::new(10.0, 10.0, 10.0), target, Vec3::Y);
    let (h, w) = (9.0, 9.0 * viewport.x / viewport.y);
    let proj = directx::orthographic(-w, w, -h, h, 100.0, 0.1);
    let ray =
        concordia_input::pointer_ray(viewport / 2.0, viewport, (proj * view).inverse()).unwrap();
    let hit = sim
        .physics
        .raycast(&ray, 200.0, LayerMask::ALL)
        .expect("ground hit");
    assert_eq!(hit.entity, ground);
    assert!(hit.point.abs_diff_eq(target, 0.01), "{}", hit.point);
}
