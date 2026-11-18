//! Pointer rays through the camera, including orthographic (M1 phase 1.1).

use concordia_core::math::camera::rh::{proj::directx, view::look_at_mat4};
use concordia_core::math::{Mat4, Vec2, Vec3};
use concordia_input::pointer_ray;

const VIEWPORT: Vec2 = Vec2::new(1280.0, 720.0);

fn iso_view() -> Mat4 {
    let eye = Vec3::new(10.0, 10.0, 10.0);
    look_at_mat4(eye, Vec3::ZERO, Vec3::Y)
}

/// Reverse-Z orthographic projection (near maps to depth 1).
fn ortho() -> Mat4 {
    let (h, w) = (10.0, 10.0 * VIEWPORT.x / VIEWPORT.y);
    directx::orthographic(-w, w, -h, h, 100.0, 0.1)
}

#[test]
fn ortho_pointer_ray_is_parallel_to_camera_forward() {
    let view = iso_view();
    let inv = (ortho() * view).inverse();
    let forward = (Vec3::ZERO - Vec3::new(10.0, 10.0, 10.0)).normalize();
    let a = pointer_ray(Vec2::new(100.0, 100.0), VIEWPORT, inv).unwrap();
    let b = pointer_ray(Vec2::new(900.0, 500.0), VIEWPORT, inv).unwrap();
    assert!(a.dir().abs_diff_eq(forward, 1e-5), "{}", a.dir());
    assert!(b.dir().abs_diff_eq(forward, 1e-5), "{}", b.dir());
    assert!(a.origin().distance(b.origin()) > 1.0);
    let center = pointer_ray(VIEWPORT / 2.0, VIEWPORT, inv).unwrap();
    let t = center.intersect_plane(Vec3::ZERO, Vec3::Y).unwrap();
    assert!(
        center.at(t).abs_diff_eq(Vec3::ZERO, 1e-3),
        "{}",
        center.at(t)
    );
}

#[test]
fn perspective_pointer_ray_passes_through_cursor_pixel_center() {
    let view = iso_view();
    let proj = directx::perspective_infinite_reverse(0.8, VIEWPORT.x / VIEWPORT.y, 0.1);
    let view_proj = proj * view;
    let target = Vec3::new(2.0, 0.5, -1.0);
    let clip = view_proj.project_point3(target);
    let pixel = Vec2::new(
        (clip.x + 1.0) * 0.5 * VIEWPORT.x,
        (1.0 - clip.y) * 0.5 * VIEWPORT.y,
    );
    let ray = pointer_ray(pixel, VIEWPORT, view_proj.inverse()).unwrap();
    let to_target = target - ray.origin();
    let off_axis = to_target - ray.dir() * to_target.dot(ray.dir());
    assert!(off_axis.length() < 1e-3, "misses by {}", off_axis.length());
    assert!(ray.origin().distance(Vec3::new(10.0, 10.0, 10.0)) < 0.2);
}

#[test]
fn degenerate_viewport_is_rejected() {
    let inv = (ortho() * iso_view()).inverse();
    assert!(pointer_ray(Vec2::ONE, Vec2::new(0.0, 720.0), inv).is_err());
    assert!(pointer_ray(Vec2::new(f32::NAN, 1.0), VIEWPORT, inv).is_err());
}
