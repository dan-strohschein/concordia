//! Convex hull and compound colliders (M8 8.0a): the shapes fracture pieces need, with the same
//! hull navigation sees.

mod common;

use common::{DT, Sim};
use concordia_core::math::Vec3;
use concordia_physics::{Collider, ColliderShape, RigidBody, hull_triangles};

fn cube(centre: Vec3, half: f32) -> Vec<Vec3> {
    (0..8)
        .map(|i| {
            let s = |bit: i32| if i & bit == 0 { -half } else { half };
            centre + Vec3::new(s(1), s(2), s(4))
        })
        .collect()
}

#[test]
fn a_convex_hull_collider_rests_like_its_box() {
    let mut sim = Sim::new();
    sim.ground();
    let hull = sim.spawn(
        Vec3::new(0.0, 3.0, 0.0),
        RigidBody::dynamic(),
        Collider::new(ColliderShape::ConvexHull {
            points: cube(Vec3::ZERO, 0.5),
        }),
    );
    let boxed = sim.cube(Vec3::new(3.0, 3.0, 0.0));
    sim.step(240);
    let (h, b) = (sim.pos(hull).y, sim.pos(boxed).y);
    assert!((h - b).abs() < 0.02, "hull {h}, box {b}");
}

#[test]
fn a_compound_collider_rests_on_its_lowest_part() {
    let mut sim = Sim::new();
    sim.ground();
    let parts = vec![cube(Vec3::ZERO, 0.5), cube(Vec3::new(0.0, 1.0, 0.0), 0.5)];
    let tower = sim.spawn(
        Vec3::new(0.0, 4.0, 0.0),
        RigidBody::dynamic(),
        Collider::new(ColliderShape::Compound { parts }),
    );
    let lone = sim.cube(Vec3::new(3.0, 4.0, 0.0));
    sim.step(240);
    let (t, c) = (sim.pos(tower).y, sim.pos(lone).y);
    assert!(
        (t - c).abs() < 0.02,
        "the lower part rests where a cube does: {t} vs {c}"
    );
}

#[test]
fn degenerate_hulls_and_compounds_are_reported_and_skipped() {
    let flat = vec![Vec3::ZERO, Vec3::X, Vec3::Z, Vec3::new(1.0, 0.0, 1.0)];
    let mut nan = cube(Vec3::ZERO, 0.5);
    nan[3].y = f32::NAN;
    let shapes = [
        ColliderShape::ConvexHull {
            points: flat.clone(),
        },
        ColliderShape::ConvexHull { points: nan },
        ColliderShape::ConvexHull {
            points: vec![Vec3::ZERO; 3],
        },
        ColliderShape::Compound { parts: vec![] },
        ColliderShape::Compound {
            parts: vec![cube(Vec3::ZERO, 0.5), flat],
        },
    ];
    let mut sim = Sim::new();
    let bad: Vec<_> = shapes
        .into_iter()
        .map(|shape| sim.spawn(Vec3::ZERO, RigidBody::dynamic(), Collider::new(shape)))
        .collect();
    let report = sim.physics.tick(&mut sim.world, DT).unwrap();
    let rejected: Vec<_> = report.rejected.iter().map(|(id, _)| *id).collect();
    assert_eq!(rejected.len(), bad.len(), "{report:?}");
    assert!(bad.iter().all(|id| rejected.contains(id)), "{report:?}");
    assert_eq!(sim.physics.body_count(), 0);
}

#[test]
fn hull_triangles_are_the_colliders_faces() {
    let tris = hull_triangles(&cube(Vec3::ZERO, 0.5)).unwrap();
    assert_eq!(tris.len(), 12);
    assert!(
        tris.iter()
            .flatten()
            .all(|v| (v.abs() - Vec3::splat(0.5)).abs().max_element() < 1e-3)
    );
    assert!(hull_triangles(&[Vec3::ZERO, Vec3::X, Vec3::Z, Vec3::ONE.with_y(0.0)]).is_err());
}
