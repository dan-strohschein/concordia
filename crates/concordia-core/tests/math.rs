//! Rays (M1 phase 1.1: pointer picking, later physics raycasts).

use concordia_core::math::{MathError, Ray, Vec3};

#[test]
fn ray_normalizes_direction_and_evaluates_points() {
    let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -2.0)).unwrap();
    assert_eq!(ray.dir(), Vec3::NEG_Z);
    assert_eq!(ray.origin(), Vec3::ZERO);
    assert_eq!(ray.at(3.0), Vec3::new(0.0, 0.0, -3.0));
}

#[test]
fn ray_rejects_zero_and_non_finite_input() {
    assert_eq!(
        Ray::new(Vec3::ZERO, Vec3::ZERO),
        Err(MathError::ZeroDirection)
    );
    assert_eq!(
        Ray::new(Vec3::splat(f32::NAN), Vec3::X),
        Err(MathError::NonFinite)
    );
    assert_eq!(
        Ray::new(Vec3::ZERO, Vec3::new(f32::INFINITY, 0.0, 0.0)),
        Err(MathError::NonFinite)
    );
}

#[test]
fn ray_hits_plane_in_front_only() {
    let ray = Ray::new(Vec3::new(0.0, 10.0, 0.0), Vec3::NEG_Y).unwrap();
    assert_eq!(ray.intersect_plane(Vec3::ZERO, Vec3::Y), Some(10.0));
    let away = Ray::new(Vec3::new(0.0, 10.0, 0.0), Vec3::Y).unwrap();
    assert_eq!(away.intersect_plane(Vec3::ZERO, Vec3::Y), None);
    let parallel = Ray::new(Vec3::new(0.0, 10.0, 0.0), Vec3::X).unwrap();
    assert_eq!(parallel.intersect_plane(Vec3::ZERO, Vec3::Y), None);
}
