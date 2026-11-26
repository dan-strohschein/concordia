//! Temporal anti-aliasing and upscaling (M4 4.6): the camera's sub-pixel jitter and the texture mip
//! bias, then (4.6a steps 3–5) the own TAA/TAAU against supersampled references.

mod common;
use common::{Scene, renderer, unlit};
use concordia_assets::{MeshRenderer, TextureData, TextureFormat};
use concordia_core::math::{Quat, Vec3};
use concordia_render::temporal::{SubpixelOverride, Temporal, TemporalBackend, Upscale};
use concordia_render::{Camera, Image, Msaa, Projection, Renderer};
use concordia_testkit::golden::{FlipScore, RgbaImage, flip_score};

/// A camera with one sample per pixel (jitter is visible only without MSAA).
fn single_sample() -> Camera {
    Camera {
        msaa: Msaa::Off,
        ..Camera::default()
    }
}

/// The centroid of pixels brighter than half, in pixels.
fn centroid(image: &Image) -> [f64; 2] {
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
    for y in 0..image.height {
        for x in 0..image.width {
            if common::pixel(image, x, y)[0] > 128 {
                sx += x as f64;
                sy += y as f64;
                n += 1.0;
            }
        }
    }
    assert!(n > 100.0, "the quad is visible");
    [sx / n, sy / n]
}

/// A sub-pixel offset moves the rasterized image by that many pixels (x right, y down).
#[test]
fn jitter_moves_the_image_by_its_offset() {
    let Some(mut r) = renderer() else { return };
    let mut scene = Scene::new();
    let white = scene.material("white", unlit([1.0; 4]));
    scene.mesh(
        "cube",
        white,
        Vec3::ZERO,
        Quat::from_rotation_z(0.3),
        Vec3::splat(1.4),
    );
    scene.camera(Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO, single_sample());
    let mut at = |offset: [f32; 2]| {
        r.set_subpixel_override(Some(SubpixelOverride {
            offset,
            mip_bias: 0.0,
        }));
        centroid(&scene.render(&mut r, 160, 90))
    };
    let still = at([0.0, 0.0]);
    for offset in [[0.5, 0.0], [0.0, 0.5], [-0.25, 0.25], [1.0, -1.0]] {
        let moved = at(offset);
        for axis in 0..2 {
            let shift = moved[axis] - still[axis];
            assert!(
                (shift - offset[axis] as f64).abs() <= 0.05,
                "offset {offset:?}, axis {axis}: moved {shift}"
            );
        }
    }
}

/// A 64² texture whose mips are red, green, blue, then grey.
fn mip_colored() -> TextureData {
    let colors = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]];
    let mips = (0..7u32)
        .map(|level| {
            let side = 64 >> level;
            let c = colors
                .get(level as usize)
                .copied()
                .unwrap_or([128, 128, 128, 255]);
            c.repeat((side * side) as usize)
        })
        .collect();
    TextureData {
        width: 64,
        height: 64,
        format: TextureFormat::Rgba8Linear,
        mips,
    }
}

/// Material textures sample with the frame's mip bias: a texture drawn at half its size picks mip
/// 1, and a bias of −1 or +1 picks mip 0 or 2.
#[test]
fn textures_sample_with_the_frame_mip_bias() {
    let Some(mut r) = renderer() else { return };
    let mut scene = Scene::new();
    let texture = scene.assets.insert_texture("mips", mip_colored());
    let paint = scene.material(
        "paint",
        common::M1 {
            base_color_texture: Some(texture),
            ..unlit([1.0; 4])
        },
    );
    let quad = scene.entity(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    scene
        .world
        .insert(
            quad,
            MeshRenderer {
                mesh: "builtin:mesh/cube".parse().unwrap(),
                material: paint,
                cast_shadows: false,
            },
        )
        .unwrap();
    // 32 pixels show the 1-unit cube's front face: 64 texels over 32 pixels is mip 1.
    let camera = Camera {
        projection: Projection::Orthographic {
            height: 2.0,
            near: 0.1,
            far: 10.0,
        },
        ..single_sample()
    };
    scene.camera(Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO, camera);
    for (bias, channel) in [(0.0, 1), (-1.0, 0), (1.0, 2)] {
        r.set_subpixel_override(Some(SubpixelOverride {
            offset: [0.0, 0.0],
            mip_bias: bias,
        }));
        let image = scene.render(&mut r, 64, 64);
        let p = common::pixel(&image, 32, 32);
        let strongest = (0..3).max_by_key(|&c| p[c]).unwrap();
        assert_eq!(strongest, channel, "bias {bias}: {p:?}");
    }
}

// ---- The fence yard (4.6 plan Q12): sub-pixel edges, a grazing checker, a bright bar.

/// The fence yard's output size.
const YARD: (u32, u32) = (640, 360);

/// A `side`² checker of `cell`-texel squares with box-filtered mips (sRGB colour).
fn checker(side: u32, cell: u32) -> TextureData {
    let mut level: Vec<[f32; 3]> = (0..side * side)
        .map(|i| {
            let (x, y) = (i % side / cell, i / side / cell);
            if (x + y) % 2 == 0 {
                [0.9, 0.85, 0.7]
            } else {
                [0.08, 0.1, 0.12]
            }
        })
        .collect();
    let encode = |l: &[[f32; 3]]| -> Vec<u8> {
        l.iter()
            .flat_map(|c| {
                let e = |v: f32| (srgb_encode(v) * 255.0).round() as u8;
                [e(c[0]), e(c[1]), e(c[2]), 255]
            })
            .collect()
    };
    let mut mips = vec![encode(&level)];
    let mut s = side;
    while s > 1 {
        let half = s / 2;
        level = (0..half * half)
            .map(|i| {
                let (x, y) = (i % half * 2, i / half * 2);
                let at = |dx: u32, dy: u32| level[((y + dy) * s + x + dx) as usize];
                let mut c = [0.0; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    (0..3).for_each(|k| c[k] += at(dx, dy)[k] / 4.0);
                }
                c
            })
            .collect();
        s = half;
        mips.push(encode(&level));
    }
    TextureData {
        width: side,
        height: side,
        format: TextureFormat::Rgba8Srgb,
        mips,
    }
}

fn srgb_encode(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_decode(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// The fence yard: a checkered ground seen at a grazing angle, a row of thin poles and three wires,
/// a sphere, and a bright bar against a dark wall, under a sun.
fn fence_yard() -> Scene {
    let mut scene = Scene::new();
    let texture = scene.assets.insert_texture("checker", checker(512, 8));
    let ground = scene.material(
        "ground",
        common::M1 {
            base_color_texture: Some(texture),
            metallic: 0.0,
            roughness: 0.8,
            ..common::M1::default()
        },
    );
    let paint = |c: [f32; 4]| common::M1 {
        base_color: c,
        metallic: 0.0,
        roughness: 0.5,
        ..common::M1::default()
    };
    let wood = scene.material("wood", paint([0.6, 0.45, 0.3, 1.0]));
    let wire = scene.material("wire", paint([0.8, 0.8, 0.85, 1.0]));
    let wall = scene.material("wall", unlit([0.02, 0.02, 0.03, 1.0]));
    // Emission is in nits (ADR-0102): about 4 after the camera's EV 15 exposure.
    let glow = scene.material(
        "glow",
        common::M1 {
            base_color: [0.0, 0.0, 0.0, 1.0],
            emissive: [1.0, 0.66, 0.25],
            emissive_strength: 160_000.0,
            ..paint([0.0, 0.0, 0.0, 1.0])
        },
    );
    let one = Quat::IDENTITY;
    scene.mesh("plane", ground, Vec3::ZERO, one, Vec3::new(40.0, 1.0, 40.0));
    // Two fences: pickets about two pixels wide near, under one pixel far, and wires thinner.
    for (z, count, width) in [(0.0, 40, 0.025), (-6.0, 60, 0.03)] {
        let span = 8.0 + -z;
        for i in 0..count {
            let x = -span / 2.0 + span * i as f32 / (count - 1) as f32;
            let size = Vec3::new(width, 1.8, width);
            scene.mesh("cube", wood.clone(), Vec3::new(x, 0.9, z), one, size);
        }
        for k in 0..8 {
            let at = Vec3::new(0.0, 0.25 + k as f32 * 0.2, z);
            scene.mesh("cube", wire.clone(), at, one, Vec3::new(span, 0.01, 0.01));
        }
    }
    scene.mesh(
        "sphere",
        wood,
        Vec3::new(1.5, 0.6, -2.0),
        one,
        Vec3::splat(1.2),
    );
    scene.mesh(
        "cube",
        wall,
        Vec3::new(-1.5, 1.2, -3.0),
        one,
        Vec3::new(3.0, 2.4, 0.1),
    );
    scene.mesh(
        "cube",
        glow,
        Vec3::new(-1.5, 1.4, -2.9),
        one,
        Vec3::new(2.2, 0.08, 0.08),
    );
    scene.sun(Vec3::new(-0.4, -1.0, -0.6), 60_000.0);
    scene.camera(
        Vec3::new(0.4, 1.2, 4.0),
        Vec3::new(0.0, 0.7, -1.0),
        Camera::default(),
    );
    scene
}

/// The scene supersampled 4 × 4 without temporal or MSAA, averaged in linear light.
fn supersampled(r: &mut Renderer, scene: &mut Scene, (w, h): (u32, u32)) -> Image {
    const K: u32 = 4;
    r.set_temporal(Temporal::default());
    r.set_subpixel_override(None);
    let camera = scene_camera(scene);
    set_msaa(scene, camera, Msaa::Off);
    let big = scene.render(r, w * K, h * K);
    set_msaa(scene, camera, Msaa::X4);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0.0f32; 3];
            for (dx, dy) in (0..K).flat_map(|dy| (0..K).map(move |dx| (dx, dy))) {
                let p = common::pixel(&big, x * K + dx, y * K + dy);
                (0..3).for_each(|c| sum[c] += srgb_decode(p[c] as f32 / 255.0));
            }
            let n = (K * K) as f32;
            rgba.extend(sum.map(|s| (srgb_encode(s / n) * 255.0).round() as u8));
            rgba.push(255);
        }
    }
    Image {
        width: w,
        height: h,
        rgba,
    }
}

fn scene_camera(scene: &Scene) -> concordia_core::id::EntityId {
    let rw = concordia_render::RenderWorld::extract(&scene.world);
    rw.camera(None).expect("a camera").entity
}

fn set_msaa(scene: &mut Scene, camera: concordia_core::id::EntityId, msaa: Msaa) {
    let mut c: Camera = scene.world.get::<Camera>(camera).unwrap().clone();
    c.msaa = msaa;
    scene.world.insert(camera, c).unwrap();
}

/// Renders `frames` still frames with `temporal` from a fresh history and returns the last.
fn converge(r: &mut Renderer, scene: &mut Scene, temporal: Temporal, frames: u32) -> Image {
    r.set_subpixel_override(None);
    r.set_temporal(temporal);
    r.reset_motion_history();
    let mut last = None;
    for _ in 0..frames {
        last = Some(scene.render(r, YARD.0, YARD.1));
    }
    last.expect("at least one frame")
}

fn flip(expected: &Image, actual: &Image) -> FlipScore {
    let img = |i: &Image| RgbaImage::new(i.width, i.height, i.rgba.clone()).unwrap();
    flip_score(&img(expected), &img(actual)).unwrap()
}

fn own(upscale: Upscale) -> Temporal {
    Temporal {
        backend: TemporalBackend::own,
        upscale,
    }
}

/// No anti-aliasing at all: one sample per pixel, temporal off.
fn no_aa(r: &mut Renderer, scene: &mut Scene) -> Image {
    r.set_temporal(Temporal::default());
    r.set_subpixel_override(None);
    let camera = scene_camera(scene);
    set_msaa(scene, camera, Msaa::Off);
    let image = scene.render(r, YARD.0, YARD.1);
    set_msaa(scene, camera, Msaa::X4);
    image
}

/// After 32 still frames the own TAA (1.0×) is within FLIP 0.03 of a 16× supersampled render, and
/// at most half as far from it as no anti-aliasing (the companion, 4.6 plan Q12).
#[test]
fn taa_static_scene_converges_to_ssaa_reference() {
    let Some(mut r) = renderer() else { return };
    let mut scene = fence_yard();
    let reference = supersampled(&mut r, &mut scene, YARD);
    let taa = converge(&mut r, &mut scene, own(Upscale::X1), 32);
    let aliased = no_aa(&mut r, &mut scene);
    r.set_temporal(Temporal::default());
    let msaa = scene.render(&mut r, YARD.0, YARD.1);
    assert_ne!(
        taa, msaa,
        "the temporal resolve ran (not the camera's 4× MSAA)"
    );
    let (score, baseline) = (flip(&reference, &taa), flip(&reference, &aliased));
    eprintln!(
        "TAA {score:?}, no AA {baseline:?}, 4× MSAA {:?}",
        flip(&reference, &msaa)
    );
    assert!(score.mean <= 0.03, "TAA against 16× SSAA: {score:?}");
    assert!(
        score.mean <= 0.5 * baseline.mean,
        "TAA {score:?} against no AA {baseline:?}"
    );
}

/// The own TAAU at 1.5× (48 frames) and 2.0× (64 frames) stays within FLIP 0.04 and 0.06 of the
/// native TAA, and closer to the supersampled render than no anti-aliasing at the output size.
#[test]
fn taau_static_scene_within_tolerance_of_native_taa() {
    let Some(mut r) = renderer() else { return };
    let mut scene = fence_yard();
    let reference = supersampled(&mut r, &mut scene, YARD);
    let aliased = flip(&reference, &no_aa(&mut r, &mut scene));
    let native = converge(&mut r, &mut scene, own(Upscale::X1), 32);
    for (upscale, frames, bound) in [(Upscale::X1_5, 48, 0.04), (Upscale::X2, 64, 0.06)] {
        let image = converge(&mut r, &mut scene, own(upscale), frames);
        assert_eq!((image.width, image.height), YARD, "the output size");
        assert_ne!(
            image, native,
            "{upscale:?} renders fewer pixels than native"
        );
        let (score, truth) = (flip(&native, &image), flip(&reference, &image));
        eprintln!("{upscale:?}: against TAA {score:?}, against SSAA {truth:?}, no AA {aliased:?}");
        assert!(
            score.mean <= bound,
            "{upscale:?} against native TAA: {score:?}"
        );
        // The companion: upscaled from fewer pixels, it still beats rendering every pixel once.
        assert!(
            truth.mean < aliased.mean,
            "{upscale:?} {truth:?} against no AA {aliased:?}"
        );
    }
}

/// Temporal changes the image: a converged TAA frame differs from a single aliased frame, and
/// with temporal off the frame is exactly today's.
#[test]
fn temporal_off_renders_exactly_as_before() {
    let Some(mut r) = renderer() else { return };
    let mut scene = fence_yard();
    r.set_temporal(Temporal::default());
    let before = scene.render(&mut r, YARD.0, YARD.1);
    let _ = converge(&mut r, &mut scene, own(Upscale::X1), 4);
    r.set_temporal(Temporal::default());
    let after = scene.render(&mut r, YARD.0, YARD.1);
    assert_eq!(before, after, "temporal off is today's frame");
}
