//! The editor's orbit camera, engine-side so native input and the web UI share one camera
//! (M1 phase 1.9d; ported from the editor UI's camera tests).

use concordia_core::math::Vec3;
use concordia_render::{OrbitCamera, Preset, Projection};

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 1e-4
}

fn cam(target: Vec3, distance: f32) -> OrbitCamera {
    OrbitCamera {
        target,
        yaw: 0.0,
        pitch: 0.0,
        distance,
        ortho: false,
    }
}

#[test]
fn the_view_looks_from_eye_to_target_at_the_orbit_distance() {
    let view = cam(Vec3::new(1.0, 2.0, 3.0), 10.0).view();
    assert!(close(view.target, Vec3::new(1.0, 2.0, 3.0)));
    assert!(close(view.eye, Vec3::new(1.0, 2.0, 13.0)));
    assert!(matches!(view.projection, Projection::Perspective { .. }));
}

#[test]
fn orbiting_keeps_the_distance_and_clamps_the_pitch_short_of_the_poles() {
    let mut c = cam(Vec3::ZERO, 5.0);
    c.orbit(300.0, -10_000.0);
    assert!((c.view().eye.length() - 5.0).abs() < 1e-4);
    assert!(c.pitch > -std::f32::consts::FRAC_PI_2);
}

#[test]
fn panning_moves_the_target_across_the_screen_not_along_the_view() {
    let mut c = cam(Vec3::ZERO, 10.0);
    c.pan(100.0, 0.0, 500.0);
    assert!(c.target.x < 0.0);
    assert!(c.target.z.abs() < 1e-5);
}

#[test]
fn zoom_scales_the_distance_and_keeps_it_positive() {
    let mut c = cam(Vec3::ZERO, 10.0);
    c.zoom(-1000.0);
    assert!(c.distance > 0.0);
    c.zoom(1.0);
    assert!(c.distance > 0.05);
}

#[test]
fn the_isometric_preset_is_orthographic_at_true_isometric_angles() {
    let mut c = cam(Vec3::ZERO, 10.0);
    c.preset(Preset::iso);
    let view = c.view();
    assert!(matches!(view.projection, Projection::Orthographic { .. }));
    let elevation = view
        .eye
        .y
        .atan2((view.eye.x * view.eye.x + view.eye.z * view.eye.z).sqrt());
    assert!((elevation.to_degrees() - 35.264).abs() < 0.01);
    c.ortho = !c.ortho;
    assert!(matches!(
        c.view().projection,
        Projection::Perspective { .. }
    ));
}

#[test]
fn bad_inputs_leave_the_camera_unchanged() {
    let mut c = cam(Vec3::ZERO, 10.0);
    let before = c.clone();
    c.orbit(f32::NAN, 1.0);
    c.pan(1.0, 1.0, 0.0);
    c.zoom(f32::INFINITY);
    assert_eq!(c, before);
}
