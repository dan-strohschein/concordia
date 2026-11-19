//! Cascaded shadow maps: splits, fitting and stabilization on the CPU (M1 phase 1.6).

use concordia_core::math::{EulerRot, Mat4, Quat, Vec3};
use concordia_render::Projection;
use concordia_render::shadow::{CascadeSettings, fit_cascades, split_distances};

#[test]
fn csm_split_distances_follow_practical_split_scheme() {
    let (near, far, n, lambda) = (0.1f32, 100.0f32, 4u8, 0.75f32);
    let splits = split_distances(near, far, n, lambda);
    assert_eq!(splits.len(), 4);
    for (i, split) in splits.iter().enumerate() {
        let t = (i + 1) as f32 / n as f32;
        let expected =
            lambda * near * (far / near).powf(t) + (1.0 - lambda) * (near + (far - near) * t);
        assert!(
            (split - expected).abs() < 1e-5 * expected.max(1.0),
            "{i}: {split} vs {expected}"
        );
    }
    assert!((splits[3] - far).abs() < 1e-4);
}

fn iso_camera(at: Vec3) -> Mat4 {
    let iso = Quat::from_euler(
        EulerRot::YXZ,
        std::f32::consts::FRAC_PI_4,
        -35.264f32.to_radians(),
        0.0,
    );
    Mat4::from_rotation_translation(iso, at + iso * Vec3::new(0.0, 0.0, 30.0))
}

const ISO: Projection = Projection::Orthographic {
    height: 20.0,
    near: 0.1,
    far: 80.0,
};
const SUN: Vec3 = Vec3::new(-0.4, -1.0, -0.3);

fn scene_bounds() -> (Vec3, Vec3) {
    (Vec3::splat(-30.0), Vec3::splat(30.0))
}

#[test]
fn csm_stabilized_cascades_snap_to_texel_grid() {
    let settings = CascadeSettings {
        cascades: 3,
        resolution: 1024,
        stabilize: true,
        ..CascadeSettings::default()
    };
    let first = fit_cascades(
        &settings,
        &ISO,
        16.0 / 9.0,
        &iso_camera(Vec3::ZERO),
        SUN,
        scene_bounds(),
    )
    .unwrap();
    for step in 1..20 {
        let moved = fit_cascades(
            &settings,
            &ISO,
            16.0 / 9.0,
            &iso_camera(Vec3::new(step as f32 * 0.0137, 0.0, step as f32 * 0.0091)),
            SUN,
            scene_bounds(),
        )
        .unwrap();
        for (a, b) in first.iter().zip(&moved) {
            assert_eq!(
                a.texel_world, b.texel_world,
                "cascade size is constant while panning"
            );
            let offset = (b.origin_light - a.origin_light) / a.texel_world;
            assert!(
                (offset.x - offset.x.round()).abs() < 1e-3
                    && (offset.y - offset.y.round()).abs() < 1e-3,
                "origin moves in whole texels: {offset}"
            );
        }
    }
}

#[test]
fn ortho_camera_csm_bounds_tightly_fit_view_volume() {
    let settings = CascadeSettings {
        cascades: 3,
        ..CascadeSettings::default()
    };
    let camera = iso_camera(Vec3::ZERO);
    let cascades = fit_cascades(&settings, &ISO, 16.0 / 9.0, &camera, SUN, scene_bounds()).unwrap();
    for c in &cascades {
        let corners = c.slice_corners;
        for corner in corners {
            let clip = c.view_proj.project_point3(corner);
            assert!(
                clip.x.abs() <= 1.0001
                    && clip.y.abs() <= 1.0001
                    && (0.0..=1.0001).contains(&clip.z),
                "corner {corner} outside cascade: {clip}"
            );
        }
        let diameter = corners
            .iter()
            .flat_map(|a| corners.iter().map(move |b| a.distance(*b)))
            .fold(0.0f32, f32::max);
        let extent = 2.0 * c.radius;
        let sphere_area = std::f32::consts::PI * (diameter / 2.0).powi(2);
        assert!(
            extent * extent <= 1.5 * sphere_area,
            "cascade {extent}² vs slice sphere area {sphere_area}"
        );
    }
}

#[test]
fn casters_outside_the_view_still_land_in_the_depth_range() {
    let settings = CascadeSettings {
        cascades: 1,
        ..CascadeSettings::default()
    };
    let cascades = fit_cascades(
        &settings,
        &ISO,
        1.0,
        &iso_camera(Vec3::ZERO),
        SUN,
        (Vec3::new(-5.0, -1.0, -5.0), Vec3::new(5.0, 40.0, 5.0)),
    )
    .unwrap();
    let tower_top = cascades[0]
        .view_proj
        .project_point3(Vec3::new(0.0, 39.0, 0.0));
    assert!(
        (0.0..=1.0).contains(&tower_top.z),
        "a tall caster above the view is inside the light's depth range: {tower_top}"
    );
}

#[test]
fn invalid_cascade_settings_are_errors() {
    for bad in [
        CascadeSettings {
            cascades: 0,
            ..CascadeSettings::default()
        },
        CascadeSettings {
            cascades: 5,
            ..CascadeSettings::default()
        },
        CascadeSettings {
            resolution: 0,
            ..CascadeSettings::default()
        },
        CascadeSettings {
            split_lambda: 1.5,
            ..CascadeSettings::default()
        },
    ] {
        assert!(
            fit_cascades(
                &bad,
                &ISO,
                1.0,
                &iso_camera(Vec3::ZERO),
                SUN,
                scene_bounds()
            )
            .is_err(),
            "{bad:?}"
        );
    }
}
