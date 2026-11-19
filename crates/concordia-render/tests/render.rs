//! Forward rendering on the GPU: goldens and invariants (M1 phase 1.5). Skips without a GPU
//! unless CONCORDIA_REQUIRE_GPU=1.

mod common;

use std::f32::consts::FRAC_PI_4;

use common::{Scene, golden, pixel, renderer, unlit};
use concordia_assets::MaterialData;
use concordia_core::math::{Quat, Vec3};
use concordia_render::{
    Camera, Msaa, Projection, RenderError, RenderRequest, RenderWorld, Tonemap,
};

fn clamp_camera(projection: Projection, msaa: Msaa) -> Camera {
    Camera {
        projection,
        msaa,
        tonemap: Tonemap::Linear,
        clear: [0.02, 0.02, 0.03, 1.0],
        ..Camera::default()
    }
}

#[test]
fn unlit_cube_golden() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let red = s.material("red", unlit([0.8, 0.1, 0.1, 1.0]));
    s.mesh(
        "cube",
        red,
        Vec3::ZERO,
        Quat::from_euler(concordia_core::math::EulerRot::YXZ, 0.6, 0.4, 0.0),
        Vec3::ONE,
    );
    s.camera(
        Vec3::new(0.0, 0.0, 4.0),
        Vec3::ZERO,
        clamp_camera(Projection::default(), Msaa::Off),
    );
    let image = s.render(&mut r, 256, 256);
    assert_eq!(
        pixel(&image, 128, 128)[..3],
        [231, 89, 89][..],
        "unlit sRGB-encodes base color 0.8/0.1/0.1"
    );
    golden("unlit_cube", &image, 0.01);
}

#[test]
fn pbr_sphere_grid_directional_golden() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    for row in 0..7 {
        for col in 0..7 {
            let material = MaterialData {
                base_color: [0.9, 0.5, 0.2, 1.0],
                metallic: row as f32 / 6.0,
                roughness: (col as f32 / 6.0).max(0.05),
                ..MaterialData::default()
            };
            let m = s.material(&format!("m{row}{col}"), material);
            let at = Vec3::new(col as f32 - 3.0, row as f32 - 3.0, 0.0) * 1.1;
            s.mesh("sphere", m, at, Quat::IDENTITY, Vec3::splat(0.5));
        }
    }
    s.sun(Vec3::new(-0.4, -0.5, -1.0), 100_000.0);
    let camera = Camera {
        projection: Projection::Orthographic {
            height: 8.2,
            near: 0.1,
            far: 50.0,
        },
        ..Camera::default()
    };
    s.camera(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, camera);
    golden(
        "pbr_sphere_grid_directional",
        &s.render(&mut r, 512, 512),
        0.01,
    );
}

#[test]
fn orthographic_isometric_camera_golden() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let grey = s.material(
        "grey",
        MaterialData {
            base_color: [0.6, 0.6, 0.6, 1.0],
            metallic: 0.0,
            roughness: 0.7,
            ..MaterialData::default()
        },
    );
    let teal = s.material(
        "teal",
        MaterialData {
            base_color: [0.1, 0.6, 0.6, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            ..MaterialData::default()
        },
    );
    for x in -3i32..=3 {
        for z in -3..=3 {
            let material = if (x + z) % 2 == 0 {
                grey.clone()
            } else {
                teal.clone()
            };
            let height = 1.0 + ((x * 7 + z * 3).rem_euclid(4)) as f32 * 0.5;
            s.mesh(
                "cube",
                material,
                Vec3::new(x as f32 * 2.0, height / 2.0, z as f32 * 2.0),
                Quat::IDENTITY,
                Vec3::new(1.6, height, 1.6),
            );
        }
    }
    s.sun(Vec3::new(-0.3, -1.0, -0.5), 100_000.0);
    let iso = Quat::from_euler(
        concordia_core::math::EulerRot::YXZ,
        FRAC_PI_4,
        -35.264f32.to_radians(),
        0.0,
    );
    let eye = iso * Vec3::new(0.0, 0.0, 30.0);
    let camera = Camera {
        projection: Projection::Orthographic {
            height: 20.0,
            near: 0.1,
            far: 100.0,
        },
        ..Camera::default()
    };
    let id = s.entity(eye, iso, Vec3::ONE);
    s.world.insert(id, camera).unwrap();
    golden("orthographic_isometric", &s.render(&mut r, 512, 384), 0.01);
}

#[test]
fn ortho_and_perspective_agree_at_center_for_matching_framing() {
    let Some(mut r) = renderer() else { return };
    let mut centers = Vec::new();
    for projection in [
        Projection::Perspective {
            fov_y: 0.8,
            near: 0.1,
            far: 100.0,
        },
        Projection::Orthographic {
            height: 4.0,
            near: 0.1,
            far: 100.0,
        },
    ] {
        let mut s = Scene::new();
        let m = s.material(
            "plane",
            MaterialData {
                base_color: [0.5, 0.5, 0.5, 1.0],
                metallic: 0.0,
                roughness: 1.0,
                ..MaterialData::default()
            },
        );
        s.mesh(
            "plane",
            m,
            Vec3::ZERO,
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            Vec3::splat(20.0),
        );
        s.sun(Vec3::new(0.3, -0.2, -1.0), 50_000.0);
        s.camera(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::ZERO,
            Camera {
                projection,
                msaa: Msaa::Off,
                ..Camera::default()
            },
        );
        centers.push(pixel(&s.render(&mut r, 64, 64), 32, 32));
    }
    for c in 0..3 {
        assert!(centers[0][c].abs_diff(centers[1][c]) <= 1, "{centers:?}");
    }
    assert!(centers[0][0] > 20, "the plane is lit: {centers:?}");
}

#[test]
fn reverse_z_has_no_z_fighting_on_distant_coplanar_quads() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let green = s.material("green", unlit([0.0, 1.0, 0.0, 1.0]));
    let red = s.material("red", unlit([1.0, 0.0, 0.0, 1.0]));
    let facing = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    s.mesh(
        "plane",
        green,
        Vec3::new(0.0, 0.0, -4999.0),
        facing,
        Vec3::splat(4000.0),
    );
    s.mesh(
        "plane",
        red,
        Vec3::new(0.0, 0.0, -5000.0),
        facing,
        Vec3::splat(4000.0),
    );
    s.camera(
        Vec3::ZERO,
        Vec3::NEG_Z,
        clamp_camera(
            Projection::Perspective {
                fov_y: 0.5,
                near: 0.1,
                far: 10_000.0,
            },
            Msaa::Off,
        ),
    );
    let image = s.render(&mut r, 128, 128);
    let red_pixels = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > p[1])
        .count();
    assert_eq!(red_pixels, 0, "the nearer quad wins everywhere at 5 km");
}

#[test]
fn msaa_resolve_matches_golden() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let white = s.material("white", unlit([1.0; 4]));
    s.mesh(
        "plane",
        white,
        Vec3::ZERO,
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_y(0.3),
        Vec3::splat(1.5),
    );
    s.camera(
        Vec3::new(0.0, 0.0, 3.0),
        Vec3::ZERO,
        Camera {
            clear: [0.0, 0.0, 0.0, 1.0],
            ..clamp_camera(Projection::default(), Msaa::X4)
        },
    );
    let image = s.render(&mut r, 128, 128);
    let partial = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 10 && p[0] < 245)
        .count();
    assert!(partial > 50, "edges have blended coverage: {partial}");
    golden("msaa_resolve", &image, 0.01);
}

/// Renders a full-view unlit plane textured with `texels` (2×2 sRGB).
fn textured_plane(r: &mut concordia_render::Renderer, texels: Vec<u8>) -> concordia_render::Image {
    let mut s = Scene::new();
    let texture = concordia_assets::TextureData {
        width: 2,
        height: 2,
        format: concordia_assets::TextureFormat::Rgba8Srgb,
        mips: vec![texels],
    };
    let t = s.assets.insert_texture("t", texture);
    let m = s.material(
        "textured",
        MaterialData {
            base_color_texture: Some(t),
            unlit: true,
            ..MaterialData::default()
        },
    );
    s.mesh(
        "plane",
        m,
        Vec3::ZERO,
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        Vec3::splat(2.0),
    );
    s.camera(
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::ZERO,
        clamp_camera(
            Projection::Orthographic {
                height: 2.0,
                near: 0.1,
                far: 10.0,
            },
            Msaa::Off,
        ),
    );
    s.render(r, 64, 64)
}

#[test]
fn base_color_texture_is_decoded_as_srgb() {
    let Some(mut r) = renderer() else { return };
    let image = textured_plane(&mut r, [128, 128, 128, 255].repeat(4));
    let center = pixel(&image, 32, 32);
    assert!(
        center[0].abs_diff(128) <= 2,
        "sRGB 128 round-trips (it would be ~188 if read as linear): {center:?}"
    );
}

#[test]
fn base_color_texture_keeps_its_orientation() {
    let Some(mut r) = renderer() else { return };
    let checker = [
        [255, 0, 0, 255],
        [0, 0, 255, 255],
        [0, 0, 255, 255],
        [255, 0, 0, 255],
    ]
    .concat();
    let image = textured_plane(&mut r, checker);
    let (top_left, top_right) = (pixel(&image, 16, 16), pixel(&image, 48, 16));
    assert!(
        top_left[0] > 240 && top_left[2] < 64,
        "top-left texel is red: {top_left:?}"
    );
    assert!(
        top_right[2] > 240 && top_right[0] < 64,
        "top-right texel is blue: {top_right:?}"
    );
}

#[test]
fn capture_twice_without_change_is_byte_identical() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let m = s.material(
        "m",
        MaterialData {
            base_color: [0.7, 0.7, 0.7, 1.0],
            metallic: 0.3,
            roughness: 0.4,
            ..MaterialData::default()
        },
    );
    s.mesh("sphere", m, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    s.sun(Vec3::new(-1.0, -1.0, -1.0), 100_000.0);
    s.camera(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, Camera::default());
    let first = s.render(&mut r, 160, 120);
    let second = s.render(&mut r, 160, 120);
    assert_eq!((first.width, first.height), (160, 120));
    assert!(
        first.rgba == second.rgba,
        "renders are deterministic on one device"
    );
}

#[test]
fn invalid_sizes_and_cameras_are_errors() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let rw = RenderWorld::extract(&s.world);
    for (width, height) in [(0, 10), (10, 0), (9000, 10)] {
        let err = r
            .render(
                &rw,
                &mut s.assets,
                &RenderRequest {
                    camera: None,
                    view: None,
                    width,
                    height,
                },
            )
            .unwrap_err();
        assert!(matches!(err, RenderError::InvalidSize { .. }), "{err}");
    }
    let err = r
        .render(
            &rw,
            &mut s.assets,
            &RenderRequest {
                camera: None,
                view: None,
                width: 8,
                height: 8,
            },
        )
        .unwrap_err();
    assert!(matches!(err, RenderError::NoCamera), "{err}");
}

#[test]
fn missing_assets_name_the_reference() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let ghost: concordia_assets::AssetRef = "asset:0193a1c2-7e40-7000-8000-00000000beef"
        .parse()
        .unwrap();
    s.mesh("cube", ghost, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    s.camera(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, Camera::default());
    let rw = RenderWorld::extract(&s.world);
    let err = r
        .render(
            &rw,
            &mut s.assets,
            &RenderRequest {
                camera: None,
                view: None,
                width: 8,
                height: 8,
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("00000000beef"), "{err}");
}

#[test]
fn gpu_tonemappers_match_the_cpu_reference() {
    let Some(mut r) = renderer() else { return };
    for tonemap in [
        Tonemap::AgX,
        Tonemap::KhronosPbrNeutral,
        Tonemap::AcesFitted,
        Tonemap::Linear,
    ] {
        for value in [0.02f32, 0.18, 0.6, 0.95] {
            let mut s = Scene::new();
            // Unlit output is display-referred, so the tonemapper sees exactly `value`.
            let m = s.material("v", unlit([value, value * 0.5, value * 0.25, 1.0]));
            s.mesh(
                "plane",
                m,
                Vec3::ZERO,
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                Vec3::splat(4.0),
            );
            s.camera(
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::ZERO,
                Camera {
                    tonemap,
                    msaa: Msaa::Off,
                    ..Camera::default()
                },
            );
            let got = pixel(&s.render(&mut r, 8, 8), 4, 4);
            let want =
                concordia_render::tonemap::apply(tonemap, [value, value * 0.5, value * 0.25])
                    .map(|c| (concordia_render::tonemap::srgb_encode(c) * 255.0).round() as u8);
            for c in 0..3 {
                assert!(
                    got[c].abs_diff(want[c]) <= 1,
                    "{tonemap:?} {value}: gpu {got:?} cpu {want:?}"
                );
            }
        }
    }
}

fn translucent(color: [f32; 3], alpha: f32) -> MaterialData {
    MaterialData {
        base_color: [color[0], color[1], color[2], alpha],
        alpha_mode: concordia_assets::AlphaMode::Blend,
        unlit: true,
        ..MaterialData::default()
    }
}

/// Two half-transparent quads over black: red in front of blue, spawned in either order.
fn layered(red_first: bool) -> concordia_render::Image {
    let Some(mut r) = renderer() else {
        panic!("checked by the caller")
    };
    let mut s = Scene::new();
    let red = s.material("red", translucent([1.0, 0.0, 0.0], 0.5));
    let blue = s.material("blue", translucent([0.0, 0.0, 1.0], 0.5));
    let facing = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let quads = [(red, 0.5), (blue, -0.5)];
    let order: Vec<_> = if red_first {
        quads.to_vec()
    } else {
        quads.iter().rev().cloned().collect()
    };
    for (m, z) in order {
        s.mesh("plane", m, Vec3::new(0.0, 0.0, z), facing, Vec3::splat(2.0));
    }
    s.camera(
        Vec3::new(0.0, 0.0, 4.0),
        Vec3::ZERO,
        clamp_camera(Projection::default(), Msaa::Off),
    );
    s.render(&mut r, 32, 32)
}

#[test]
fn alpha_blend_sorts_back_to_front() {
    if renderer().is_none() {
        return;
    }
    let (a, b) = (layered(true), layered(false));
    assert_eq!(a.rgba, b.rgba, "spawn order does not matter");
    let p = pixel(&a, 16, 16);
    // Over black: blue·0.5 first, then red·0.5 over it → linear (0.5, 0, 0.25 + ~0.02 clear).
    assert!(
        p[0] > 170 && p[2] > 120 && p[2] < 150,
        "red over blue: {p:?}"
    );
}

#[test]
fn alpha_mask_discards_below_cutoff() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let masked = s.material(
        "masked",
        MaterialData {
            base_color: [1.0, 1.0, 1.0, 0.3],
            alpha_mode: concordia_assets::AlphaMode::Mask { cutoff: 0.5 },
            unlit: true,
            ..MaterialData::default()
        },
    );
    s.mesh(
        "plane",
        masked,
        Vec3::ZERO,
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        Vec3::splat(2.0),
    );
    s.camera(
        Vec3::new(0.0, 0.0, 4.0),
        Vec3::ZERO,
        clamp_camera(Projection::default(), Msaa::Off),
    );
    let p = pixel(&s.render(&mut r, 16, 16), 8, 8);
    assert!(
        p[0] < 60,
        "alpha 0.3 < cutoff 0.5 is discarded, leaving the clear color (white would be 255): {p:?}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn renderer_presents_frames_to_a_window_surface() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let red = s.material("red", unlit([0.8, 0.1, 0.1, 1.0]));
    s.mesh("cube", red, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    s.camera(
        Vec3::new(0.0, 0.0, 4.0),
        Vec3::ZERO,
        clamp_camera(Projection::default(), Msaa::X4),
    );
    let layer = objc2_quartz_core::CAMetalLayer::new();
    // SAFETY: the layer outlives the surface, which the renderer releases when dropped.
    let surface = unsafe {
        r.create_surface(concordia_rhi::SurfaceTarget::MetalLayer(
            std::ptr::NonNull::from(&*layer).cast(),
        ))
    }
    .unwrap();
    let rw = RenderWorld::extract(&s.world);
    let request = RenderRequest {
        camera: None,
        view: None,
        width: 96,
        height: 64,
    };
    let presented = (0..10)
        .filter(|_| r.present(&rw, &mut s.assets, &request, surface).unwrap())
        .count();
    assert!(presented >= 8, "presented {presented} of 10 frames");
    let image = r.render(&rw, &mut s.assets, &request).unwrap();
    assert_eq!(
        pixel(&image, 48, 32)[..3],
        [231, 89, 89][..],
        "offscreen rendering still works alongside"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn presented_frames_overlap_the_gpu_and_retire_their_memory() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let red = s.material("red", unlit([0.8, 0.1, 0.1, 1.0]));
    s.mesh("cube", red, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    s.camera(
        Vec3::new(0.0, 0.0, 4.0),
        Vec3::ZERO,
        clamp_camera(Projection::default(), Msaa::X4),
    );
    let layer = objc2_quartz_core::CAMetalLayer::new();
    // SAFETY: the layer outlives the surface, which the renderer releases when dropped.
    let surface = unsafe {
        r.create_surface(concordia_rhi::SurfaceTarget::MetalLayer(
            std::ptr::NonNull::from(&*layer).cast(),
        ))
    }
    .unwrap();
    let rw = RenderWorld::extract(&s.world);
    let request = RenderRequest {
        camera: None,
        view: None,
        width: 512,
        height: 512,
    };
    assert!(r.present(&rw, &mut s.assets, &request, surface).unwrap());
    assert_eq!(
        r.frames_in_flight(),
        1,
        "presenting returns without waiting for the GPU"
    );
    for _ in 0..5 {
        r.present(&rw, &mut s.assets, &request, surface).unwrap();
    }
    let warm = r.allocated_bytes().expect("Metal reports allocations");
    for _ in 0..40 {
        r.present(&rw, &mut s.assets, &request, surface).unwrap();
        assert!(
            r.frames_in_flight() <= 2,
            "the default keeps at most two frames in flight"
        );
    }
    let grown = r.allocated_bytes().unwrap().saturating_sub(warm);
    assert!(
        grown < 64 << 20,
        "40 more frames grew GPU memory by {} MB",
        grown >> 20
    );
    let image = r.render(&rw, &mut s.assets, &request).unwrap();
    assert_eq!(
        r.frames_in_flight(),
        0,
        "a readback finishes every earlier frame"
    );
    assert_eq!(pixel(&image, 256, 256)[..3], [231, 89, 89][..]);
}

#[cfg(target_os = "macos")]
#[test]
fn frames_in_flight_can_be_lowered_for_latency() {
    let Some(mut r) = renderer() else { return };
    assert!(r.set_frames_in_flight(0).is_err());
    assert!(r.set_frames_in_flight(4).is_err());
    r.set_frames_in_flight(1).unwrap();
    let mut s = Scene::new();
    let red = s.material("red", unlit([0.8, 0.1, 0.1, 1.0]));
    s.mesh("cube", red, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    s.camera(
        Vec3::new(0.0, 0.0, 4.0),
        Vec3::ZERO,
        clamp_camera(Projection::default(), Msaa::X4),
    );
    let layer = objc2_quartz_core::CAMetalLayer::new();
    // SAFETY: the layer outlives the surface, which the renderer releases when dropped.
    let surface = unsafe {
        r.create_surface(concordia_rhi::SurfaceTarget::MetalLayer(
            std::ptr::NonNull::from(&*layer).cast(),
        ))
    }
    .unwrap();
    let rw = RenderWorld::extract(&s.world);
    let request = RenderRequest {
        camera: None,
        view: None,
        width: 128,
        height: 128,
    };
    for _ in 0..10 {
        r.present(&rw, &mut s.assets, &request, surface).unwrap();
        assert!(r.frames_in_flight() <= 1);
    }
}

#[test]
fn off_screen_meshes_are_culled_and_repeated_meshes_draw_as_one_instanced_call() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let red = s.material("red", unlit([0.8, 0.1, 0.1, 1.0]));
    for i in 0..10 {
        s.mesh(
            "cube",
            red.clone(),
            Vec3::new(i as f32 * 0.3 - 1.5, 0.0, 0.0),
            Quat::IDENTITY,
            Vec3::splat(0.2),
        );
    }
    for i in 0..3 {
        s.mesh(
            "cube",
            red.clone(),
            Vec3::new(i as f32, 0.0, 20.0),
            Quat::IDENTITY,
            Vec3::ONE,
        );
    }
    s.camera(
        Vec3::new(0.0, 0.0, 4.0),
        Vec3::ZERO,
        clamp_camera(Projection::default(), Msaa::X4),
    );
    let rw = RenderWorld::extract(&s.world);
    let request = RenderRequest {
        camera: None,
        view: None,
        width: 96,
        height: 64,
    };
    let image = r.render(&rw, &mut s.assets, &request).unwrap();
    let stats = r.stats();
    assert_eq!((stats.draws, stats.culled), (10, 3), "{stats:?}");
    assert_eq!(
        stats.draw_calls, 1,
        "ten identical cubes are one instanced draw: {stats:?}"
    );
    assert_eq!(pixel(&image, 48, 32)[..3], [231, 89, 89][..]);
}

#[cfg(target_os = "macos")]
#[test]
fn warm_frames_reuse_their_gpu_memory_instead_of_allocating() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let red = s.material("red", unlit([0.8, 0.1, 0.1, 1.0]));
    s.mesh("cube", red, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    let lamp = s.entity(Vec3::new(1.0, 1.0, 2.0), Quat::IDENTITY, Vec3::ONE);
    s.world
        .insert(lamp, concordia_render::PointLight::default())
        .unwrap();
    s.camera(
        Vec3::new(0.0, 0.0, 4.0),
        Vec3::ZERO,
        clamp_camera(Projection::default(), Msaa::X4),
    );
    let layer = objc2_quartz_core::CAMetalLayer::new();
    // SAFETY: the layer outlives the surface, which the renderer releases when dropped.
    let surface = unsafe {
        r.create_surface(concordia_rhi::SurfaceTarget::MetalLayer(
            std::ptr::NonNull::from(&*layer).cast(),
        ))
    }
    .unwrap();
    let rw = RenderWorld::extract(&s.world);
    let request = RenderRequest {
        camera: None,
        view: None,
        width: 256,
        height: 256,
    };
    for _ in 0..6 {
        r.present(&rw, &mut s.assets, &request, surface).unwrap();
    }
    for frame in 0..6 {
        r.present(&rw, &mut s.assets, &request, surface).unwrap();
        assert_eq!(
            r.stats().allocations,
            0,
            "warm frame {frame} created GPU objects"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_drawable_acquired_ahead_is_used_by_the_next_present() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let red = s.material("red", unlit([0.8, 0.1, 0.1, 1.0]));
    s.mesh("cube", red, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    s.camera(
        Vec3::new(0.0, 0.0, 4.0),
        Vec3::ZERO,
        clamp_camera(Projection::default(), Msaa::X4),
    );
    let layer = objc2_quartz_core::CAMetalLayer::new();
    // SAFETY: the layer outlives the surface, which the renderer releases when dropped.
    let surface = unsafe {
        r.create_surface(concordia_rhi::SurfaceTarget::MetalLayer(
            std::ptr::NonNull::from(&*layer).cast(),
        ))
    }
    .unwrap();
    let rw = RenderWorld::extract(&s.world);
    let request = |w, h| RenderRequest {
        camera: None,
        view: None,
        width: w,
        height: h,
    };
    assert!(r.acquire_ahead(surface, 64, 64).unwrap());
    assert!(
        r.acquire_ahead(surface, 64, 64).unwrap(),
        "holding one already is fine"
    );
    assert!(
        r.present(&rw, &mut s.assets, &request(64, 64), surface)
            .unwrap()
    );
    assert!(r.acquire_ahead(surface, 64, 64).unwrap());
    assert!(
        r.present(&rw, &mut s.assets, &request(32, 32), surface)
            .unwrap(),
        "a size change re-acquires"
    );
    for _ in 0..4 {
        r.acquire_ahead(surface, 48, 48).unwrap();
        r.present(&rw, &mut s.assets, &request(48, 48), surface)
            .unwrap();
    }
}
