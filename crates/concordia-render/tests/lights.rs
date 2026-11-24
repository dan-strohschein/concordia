//! Point and spot lights through clustered forward shading on the GPU (M1 phase 1.6). Skips
//! without a GPU unless CONCORDIA_REQUIRE_GPU=1.

mod common;

use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

use common::{M1, Scene, golden, pixel, renderer};
use concordia_core::math::{EulerRot, Quat, Vec3};
use concordia_render::cluster::{ClusterCuller, ClusterGrid, ClusterSettings, LightBounds};
use concordia_render::{Camera, Msaa, PointLight, Projection, SpotLight, Tonemap};
use concordia_shaders::{SlangToolchain, SlangcCompiler};
use concordia_testkit::gpu_required;

fn culler() -> Option<ClusterCuller> {
    let device = match concordia_rhi_metal::new_headless() {
        Ok(device) => Arc::<dyn concordia_rhi::Device>::from(device),
        Err(concordia_rhi::RhiError::NoDevice { reason }) => {
            assert!(
                !gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            return None;
        }
        Err(other) => panic!("{other}"),
    };
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    Some(ClusterCuller::new(device, &compiler).expect("culler"))
}

/// Deterministic pseudo-random lights spread around and beyond the view volume.
fn random_lights(n: usize) -> Vec<LightBounds> {
    let mut state = 0x2545_f491_u32;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as f32 / u32::MAX as f32
    };
    (0..n)
        .map(|_| LightBounds {
            center: Vec3::new(
                next() * 80.0 - 40.0,
                next() * 50.0 - 25.0,
                -next() * 90.0 + 5.0,
            ),
            radius: 0.5 + next() * 6.0,
        })
        .collect()
}

#[test]
fn cluster_assignment_matches_cpu_reference() {
    let Some(culler) = culler() else { return };
    let lights = random_lights(1024);
    for projection in [
        Projection::Perspective {
            fov_y: 1.0,
            near: 0.1,
            far: 100.0,
        },
        Projection::Orthographic {
            height: 30.0,
            near: 1.0,
            far: 90.0,
        },
    ] {
        let grid = ClusterGrid::new(ClusterSettings::default(), &projection, 16.0 / 9.0).unwrap();
        let gpu = culler.cull(&grid, &lights).unwrap();
        let cpu = grid.assign(&lights);
        assert_eq!(gpu.overflow, cpu.overflow, "{projection:?}");
        let differing = (0..grid.len())
            .filter(|&c| gpu.lists[c] != cpu.lists[c])
            .count();
        assert_eq!(differing, 0, "{projection:?}: {differing} clusters differ");
        assert!(
            cpu.lists.iter().any(|l| l.len() > 3),
            "the test exercises multi-light clusters"
        );
    }
}

#[test]
fn cluster_overflow_is_reported_not_silent() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let grey = s.material(
        "grey",
        M1 {
            metallic: 0.0,
            ..M1::default()
        },
    );
    s.mesh("plane", grey, Vec3::ZERO, Quat::IDENTITY, Vec3::splat(10.0));
    for i in 0..200 {
        let id = s.entity(
            Vec3::new(0.0, 1.0 + i as f32 * 0.001, 0.0),
            Quat::IDENTITY,
            Vec3::ONE,
        );
        s.world
            .insert(
                id,
                PointLight {
                    range: 3.0,
                    ..PointLight::default()
                },
            )
            .unwrap();
    }
    s.camera(
        Vec3::new(0.0, 8.0, 0.01),
        Vec3::ZERO,
        Camera {
            exposure_ev100: 8.0,
            ..Camera::default()
        },
    );
    s.render(&mut r, 64, 64);
    let stats = r.stats();
    assert_eq!(stats.punctual_lights, 200);
    assert!(stats.cluster_overflow > 0, "{stats:?}");
}

#[test]
fn point_light_follows_the_inverse_square_law() {
    let Some(mut r) = renderer() else { return };
    let mut center = Vec::new();
    for height in [2.0f32, 4.0] {
        let mut s = Scene::new();
        let white = s.material(
            "white",
            M1 {
                base_color: [1.0; 4],
                metallic: 0.0,
                roughness: 1.0,
                ..M1::default()
            },
        );
        s.mesh(
            "plane",
            white,
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec3::splat(10.0),
        );
        let lamp = s.entity(Vec3::new(0.0, height, 0.0), Quat::IDENTITY, Vec3::ONE);
        s.world
            .insert(
                lamp,
                PointLight {
                    intensity_lumens: 800.0,
                    range: 50.0,
                    ..PointLight::default()
                },
            )
            .unwrap();
        let camera = Camera {
            exposure_ev100: 3.0,
            tonemap: Tonemap::Linear,
            msaa: Msaa::Off,
            ..Camera::default()
        };
        s.camera(Vec3::new(0.0, 6.0, 0.001), Vec3::ZERO, camera);
        let p = pixel(&s.render(&mut r, 32, 32), 16, 16);
        center.push(srgb_decode(p[0]));
    }
    let ratio = center[0] / center[1];
    assert!(
        (ratio - 4.0).abs() < 0.25,
        "twice the height, a quarter the light: {center:?} ratio {ratio}"
    );
}

fn srgb_decode(v: u8) -> f32 {
    let x = v as f32 / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

#[test]
fn many_lights_golden() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let floor = s.material(
        "floor",
        M1 {
            base_color: [0.8, 0.8, 0.8, 1.0],
            metallic: 0.0,
            roughness: 0.6,
            ..M1::default()
        },
    );
    s.mesh(
        "plane",
        floor,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::splat(40.0),
    );
    for i in 0..512u32 {
        let (x, z) = ((i % 32) as f32 - 15.5, (i / 32) as f32 - 7.5);
        let hue = i as f32 / 512.0 * std::f32::consts::TAU;
        let color = [
            0.5 + 0.5 * hue.cos(),
            0.5 + 0.5 * (hue + 2.1).cos(),
            0.5 + 0.5 * (hue + 4.2).cos(),
        ];
        let id = s.entity(Vec3::new(x * 1.2, 0.4, z * 2.4), Quat::IDENTITY, Vec3::ONE);
        s.world
            .insert(
                id,
                PointLight {
                    color,
                    intensity_lumens: 60.0,
                    range: 2.5,
                },
            )
            .unwrap();
    }
    let iso = Quat::from_euler(
        EulerRot::YXZ,
        std::f32::consts::FRAC_PI_4,
        -35.264f32.to_radians(),
        0.0,
    );
    let camera = Camera {
        projection: Projection::Orthographic {
            height: 24.0,
            near: 0.1,
            far: 100.0,
        },
        exposure_ev100: 4.0,
        ..Camera::default()
    };
    let id = s.entity(iso * Vec3::new(0.0, 0.0, 40.0), iso, Vec3::ONE);
    s.world.insert(id, camera).unwrap();
    golden("many_lights_iso", &s.render(&mut r, 512, 288));
}

#[test]
fn spot_light_cone_falloff_golden() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let floor = s.material(
        "floor",
        M1 {
            base_color: [0.9, 0.9, 0.9, 1.0],
            metallic: 0.0,
            roughness: 0.8,
            ..M1::default()
        },
    );
    s.mesh(
        "plane",
        floor,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::splat(20.0),
    );
    let spot = s.entity(
        Vec3::new(0.0, 4.0, 0.0),
        Quat::from_rotation_x(-FRAC_PI_2),
        Vec3::ONE,
    );
    s.world
        .insert(
            spot,
            SpotLight {
                color: [1.0, 0.85, 0.6],
                intensity_lumens: 8000.0,
                range: 12.0,
                inner_angle: 0.35,
                outer_angle: 0.55,
            },
        )
        .unwrap();
    s.camera(
        Vec3::new(0.0, 9.0, 6.0),
        Vec3::ZERO,
        Camera {
            exposure_ev100: 5.0,
            ..Camera::default()
        },
    );
    let image = s.render(&mut r, 256, 256);
    let (inside, outside) = (pixel(&image, 128, 150)[0], pixel(&image, 10, 245)[0]);
    assert!(
        inside > 100 && outside < 10,
        "lit inside the cone, dark outside: {inside} {outside}"
    );
    golden("spot_light_cone", &image);
}
