//! Cameras: reverse-Z projections and views (M1 phase 1.5).

use concordia_core::math::{Mat4, Quat, Vec3, Vec4};
use concordia_render::{Camera, Projection};

fn depth(proj: Mat4, view_z: f32) -> f32 {
    let clip = proj * Vec4::new(0.0, 0.0, view_z, 1.0);
    clip.z / clip.w
}

#[test]
fn perspective_is_reverse_z() {
    let p = Projection::Perspective {
        fov_y: 1.0,
        near: 0.1,
        far: 100.0,
    }
    .matrix(16.0 / 9.0)
    .unwrap();
    assert!((depth(p, -0.1) - 1.0).abs() < 1e-5, "near maps to 1");
    assert!(depth(p, -100.0).abs() < 1e-5, "far maps to 0");
    assert!(depth(p, -1.0) > depth(p, -2.0), "nearer is larger");
}

#[test]
fn orthographic_is_reverse_z_and_height_is_world_units() {
    let p = Projection::Orthographic {
        height: 20.0,
        near: 0.0,
        far: 50.0,
    }
    .matrix(2.0)
    .unwrap();
    assert!((depth(p, 0.0) - 1.0).abs() < 1e-6 && depth(p, -50.0).abs() < 1e-6);
    let top = p * Vec4::new(0.0, 10.0, -1.0, 1.0);
    let right = p * Vec4::new(20.0, 0.0, -1.0, 1.0);
    assert!(
        (top.y - 1.0).abs() < 1e-6,
        "half the height reaches the top edge"
    );
    assert!((right.x - 1.0).abs() < 1e-6, "width is height × aspect");
}

#[test]
fn invalid_projections_are_errors_not_panics() {
    for bad in [
        Projection::Perspective {
            fov_y: 0.0,
            near: 0.1,
            far: 10.0,
        },
        Projection::Perspective {
            fov_y: 1.0,
            near: 0.0,
            far: 10.0,
        },
        Projection::Perspective {
            fov_y: 1.0,
            near: 5.0,
            far: 1.0,
        },
        Projection::Orthographic {
            height: -1.0,
            near: 0.0,
            far: 10.0,
        },
        Projection::Orthographic {
            height: 1.0,
            near: f32::NAN,
            far: 10.0,
        },
    ] {
        assert!(bad.matrix(1.0).is_err(), "{bad:?}");
    }
    assert!(Projection::default().matrix(0.0).is_err(), "zero aspect");
}

#[test]
fn view_ignores_scale_and_looks_down_negative_z() {
    let world = Mat4::from_scale_rotation_translation(
        Vec3::splat(3.0),
        Quat::from_rotation_y(0.5),
        Vec3::new(1.0, 2.0, 3.0),
    );
    let view = concordia_render::view_matrix(&world);
    let ahead = world.transform_point3(Vec3::ZERO) + Quat::from_rotation_y(0.5) * Vec3::NEG_Z * 4.0;
    let in_view = view.transform_point3(ahead);
    assert!(
        in_view.abs_diff_eq(Vec3::new(0.0, 0.0, -4.0), 1e-5),
        "{in_view}"
    );
}

#[test]
fn camera_defaults_are_sensible_and_round_trip_as_canonical_ron() {
    let camera = Camera::default();
    assert!(matches!(camera.projection, Projection::Perspective { .. }));
    let text = concordia_scene::write_document(&camera).unwrap();
    assert_eq!(
        concordia_scene::read_document::<Camera>(&text).unwrap(),
        camera
    );
}
