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

/// The goldens' suite.
const SUITE: &str = "m4_temporal";

/// Checks each `(name, image)` against its `m4_temporal` golden, then fails naming each miss.
fn goldens(images: &[(&str, &Image)]) {
    common::assert_goldens_in(SUITE, images);
}

/// Keeps frame `i` of sequence `name` for the review page's videos when `CONCORDIA_REVIEW_DIR` is
/// set (binary PPM; `ffmpeg` turns a sequence into a loop).
fn review(name: &str, i: u32, image: &Image) {
    let Ok(dir) = std::env::var("CONCORDIA_REVIEW_DIR") else {
        return;
    };
    let dir = std::path::Path::new(&dir).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let mut ppm = format!("P6 {} {} 255\n", image.width, image.height).into_bytes();
    image.rgba.chunks(4).for_each(|p| ppm.extend(&p[..3]));
    std::fs::write(dir.join(format!("{i:03}.ppm")), ppm).unwrap();
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
    let big = plain_frame(r, scene, (w * K, h * K));
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
    plain_frame(r, scene, YARD)
}

/// The offscreen view reference frames render in, so the temporal view's history goes on.
const REFERENCE_VIEW: u64 = 99;

/// One sample per pixel, temporal off, at `size`, in its own view; the settings are restored.
fn plain_frame(r: &mut Renderer, scene: &mut Scene, (w, h): (u32, u32)) -> Image {
    let temporal = r.temporal();
    r.set_temporal(Temporal::default());
    r.set_subpixel_override(None);
    r.select_history(REFERENCE_VIEW);
    let camera = scene_camera(scene);
    set_msaa(scene, camera, Msaa::Off);
    let image = scene.render(r, w, h);
    set_msaa(scene, camera, Msaa::X4);
    r.select_history(0);
    r.set_temporal(temporal);
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
    review("fence_ssaa", 0, &reference);
    goldens(&[("fence_taa", &taa), ("fence_no_aa", &aliased)]);
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
    let mut upscaled = Vec::new();
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
        upscaled.push(image);
    }
    goldens(&[
        ("fence_taau_1_5x", &upscaled[0]),
        ("fence_taau_2x", &upscaled[1]),
    ]);
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

// ---- Motion and robustness (4.6 plan Q12).

/// Mean luminance (display-linear, 0–1) of `image` minus `truth` over `mask`'s pixels.
fn mean_luma_bias(truth: &Image, image: &Image, mask: &[bool]) -> f32 {
    let luma = |i: &Image, k: usize| {
        let p = &i.rgba[k * 4..k * 4 + 3];
        let l = |c: u8| srgb_decode(c as f32 / 255.0);
        0.2126 * l(p[0]) + 0.7152 * l(p[1]) + 0.0722 * l(p[2])
    };
    let picked: Vec<usize> = (0..mask.len()).filter(|&k| mask[k]).collect();
    assert!(picked.len() > 50, "the region has pixels: {}", picked.len());
    picked
        .iter()
        .map(|&k| luma(image, k) - luma(truth, k))
        .sum::<f32>()
        / picked.len() as f32
}

/// Pixels that differ clearly between `a` and `b`.
fn changed(a: &Image, b: &Image) -> Vec<bool> {
    a.rgba
        .chunks(4)
        .zip(b.rgba.chunks(4))
        .map(|(p, q)| (0..3).any(|c| (p[c] as i32 - q[c] as i32).abs() > 40))
        .collect()
}

/// The camera slides sideways about 2 output pixels a frame for 24 frames: the last frame is within
/// FLIP 0.05 of the supersampled render of its pose (reprojection keeps the image).
#[test]
fn taa_panning_camera_converges() {
    let Some(mut r) = renderer() else { return };
    let score = pan(&mut r, own(Upscale::X1), "panning");
    eprintln!("panning: {score:?}");
    assert!(score.mean <= 0.05, "panning against SSAA: {score:?}");
}

/// Pans the fence yard 24 frames with `temporal`; the last frame's FLIP against the supersampled
/// render of its pose (frames kept for review under `name`).
fn pan(r: &mut Renderer, temporal: Temporal, name: &str) -> FlipScore {
    let mut scene = fence_yard();
    let camera = scene_camera(&scene);
    let (eye, target) = (Vec3::new(0.4, 1.2, 4.0), Vec3::new(0.0, 0.7, -1.0));
    let step = Vec3::new(0.025, 0.0, 0.0);
    r.set_temporal(temporal);
    r.reset_motion_history();
    let mut last = None;
    for f in 0..24 {
        scene.aim(camera, eye + step * f as f32, target + step * f as f32);
        let image = scene.render(r, YARD.0, YARD.1);
        review(name, f, &image);
        last = Some(image);
    }
    assert_eq!(
        r.stats().temporal.backend,
        temporal.backend,
        "the backend ran"
    );
    let reference = supersampled(r, &mut scene, YARD);
    flip(&reference, &last.unwrap())
}

/// A bright panel slides away fast from in front of the dark wall and the sky: 4 frames later the
/// uncovered region shows no trail of it (mean luminance within 0.02 of the plain frame), and with
/// the disocclusion test off it does (the companion: at least twice that).
#[test]
fn taa_no_ghosting_after_disocclusion() {
    let Some(mut r) = renderer() else { return };
    let mut run = |disocclusion: bool| {
        let mut scene = fence_yard();
        let white = scene.material("panel", unlit([0.9, 0.9, 0.9, 1.0]));
        let one = Quat::IDENTITY;
        let size = Vec3::new(1.6, 1.6, 0.05);
        let panel = scene.mesh("cube", white, Vec3::new(-1.5, 1.6, -1.0), one, size);
        let place = |scene: &mut Scene, x: f32| {
            let t = concordia_scene::builtin::Transform {
                translation: Vec3::new(x, 1.6, -1.0),
                rotation: one,
                scale: size,
            };
            scene.world.insert(panel, t).unwrap();
        };
        r.set_temporal_switches(concordia_render::temporal::TemporalSwitches {
            disocclusion,
            reactive: true,
        });
        r.set_temporal(own(Upscale::X1));
        r.reset_motion_history();
        for _ in 0..16 {
            scene.render(&mut r, YARD.0, YARD.1);
        }
        let covered = no_aa(&mut r, &mut scene);
        r.set_temporal(own(Upscale::X1));
        let mut image = None;
        for f in 1..=4 {
            place(&mut scene, -1.5 + 0.6 * f as f32);
            let frame = scene.render(&mut r, YARD.0, YARD.1);
            review(&format!("disocclusion_{disocclusion}"), f, &frame);
            image = Some(frame);
        }
        let plain = no_aa(&mut r, &mut scene);
        // Uncovered: the panel was there and isn't now.
        let mask: Vec<bool> = changed(&covered, &plain)
            .iter()
            .zip(plain.rgba.chunks(4))
            .map(|(c, p)| *c && p[0] < 100)
            .collect();
        let image = image.unwrap();
        (mean_luma_bias(&plain, &image, &mask), image)
    };
    let ((on, image), (off, _)) = (run(true), run(false));
    eprintln!("ghost: detection on {on}, off {off}");
    assert!(on.abs() <= 0.02, "a trail with the disocclusion test: {on}");
    assert!(off >= 0.04, "the test sees a trail without it: {off}");
    goldens(&[("disocclusion_frame_4", &image)]);
}

/// A bright translucent spark (a stand-in for M11's particles: blended draws write no motion)
/// sweeps across the dark wall: the reactive mask keeps it from smearing (the trail behind it within
/// 0.02 of the plain frame), and without the mask it smears (at least three times that).
#[test]
fn taa_reactive_mask_prevents_translucent_smearing() {
    let Some(mut r) = renderer() else { return };
    let mut run = |reactive: bool| {
        let mut scene = fence_yard();
        let spark = scene.material(
            "spark",
            common::M1 {
                base_color: [1.0, 0.8, 0.4, 0.7],
                alpha_mode: concordia_assets::AlphaMode::Blend,
                ..unlit([1.0; 4])
            },
        );
        let one = Quat::IDENTITY;
        let size = Vec3::new(0.4, 0.4, 0.02);
        let id = scene.mesh("cube", spark, Vec3::new(-2.6, 1.0, -2.8), one, size);
        r.set_temporal_switches(concordia_render::temporal::TemporalSwitches {
            disocclusion: true,
            reactive,
        });
        r.set_temporal(own(Upscale::X1));
        r.reset_motion_history();
        let mut image = None;
        let mut before = None;
        for f in 0..24 {
            let t = concordia_scene::builtin::Transform {
                translation: Vec3::new(-2.6 + 0.12 * f as f32, 1.0, -2.8),
                rotation: one,
                scale: size,
            };
            scene.world.insert(id, t).unwrap();
            if f == 16 {
                before = Some(no_aa(&mut r, &mut scene));
                r.set_temporal(own(Upscale::X1));
            }
            let frame = scene.render(&mut r, YARD.0, YARD.1);
            review(&format!("reactive_{reactive}"), f, &frame);
            image = Some(frame);
        }
        let plain = no_aa(&mut r, &mut scene);
        // The trail: where the spark was at frame 16 and isn't now.
        let mask: Vec<bool> = changed(before.as_ref().unwrap(), &plain)
            .iter()
            .zip(before.unwrap().rgba.chunks(4))
            .zip(plain.rgba.chunks(4))
            .map(|((c, b), p)| *c && b[0] > p[0])
            .collect();
        let image = image.unwrap();
        (mean_luma_bias(&plain, &image, &mask), image)
    };
    let ((on, image), (off, _)) = (run(true), run(false));
    eprintln!("smear: reactive on {on}, off {off}");
    assert!(on.abs() <= 0.02, "a trail with the reactive mask: {on}");
    assert!(off >= 0.06, "the test sees a smear without it: {off}");
    goldens(&[("reactive_frame_24", &image)]);
}

/// One EV step brightens the next frame at once: its mean luminance is within 2 % of a converged
/// render at the new exposure (history made at the old exposure is rescaled, not faded in).
#[test]
fn exposure_change_does_not_flash() {
    let Some(mut r) = renderer() else { return };
    let (a, b) = exposure_step(&mut r, own(Upscale::X1));
    eprintln!("exposure: next {a}, settled {b}");
    assert!(
        (a - b).abs() <= 0.02 * b,
        "the frame after the change: {a} against {b}"
    );
}

/// The mean luminance of the frame after one EV step with `temporal`, and of a converged render at
/// the new exposure.
fn exposure_step(r: &mut Renderer, temporal: Temporal) -> (f32, f32) {
    let mut scene = fence_yard();
    let camera = scene_camera(&scene);
    let set_ev = |scene: &mut Scene, ev: f32| {
        let mut c: Camera = scene.world.get::<Camera>(camera).unwrap().clone();
        c.exposure_ev100 = ev;
        scene.world.insert(camera, c).unwrap();
    };
    let _ = converge(r, &mut scene, temporal, 16);
    set_ev(&mut scene, 14.0);
    let next = scene.render(r, YARD.0, YARD.1);
    assert_eq!(
        r.stats().temporal.backend,
        temporal.backend,
        "the backend ran"
    );
    let settled = converge(r, &mut scene, temporal, 16);
    let all = vec![true; (YARD.0 * YARD.1) as usize];
    let mean = |i: &Image| {
        mean_luma_bias(
            &Image {
                rgba: vec![0; i.rgba.len()],
                ..i.clone()
            },
            i,
            &all,
        )
    };
    (mean(&next), mean(&settled))
}

/// Two views rendered in turn each keep their own history: a view's frames equal its frames
/// rendered alone.
#[test]
fn history_is_per_view() {
    let Some(mut r) = renderer() else { return };
    let mut scene = fence_yard();
    let camera = scene_camera(&scene);
    let poses = [
        (Vec3::new(0.4, 1.2, 4.0), Vec3::new(0.0, 0.7, -1.0)),
        (Vec3::new(-2.0, 2.0, 3.0), Vec3::new(1.0, 0.5, -2.0)),
    ];
    r.set_temporal(own(Upscale::X1));
    let mut alone = None;
    r.select_history(10);
    for _ in 0..6 {
        scene.aim(camera, poses[0].0, poses[0].1);
        alone = Some(scene.render(&mut r, YARD.0, YARD.1));
    }
    let mut shared = None;
    for _ in 0..6 {
        for (view, (eye, target)) in poses.iter().enumerate() {
            r.select_history(20 + view as u64);
            scene.aim(camera, *eye, *target);
            let image = scene.render(&mut r, YARD.0, YARD.1);
            if view == 0 {
                shared = Some(image);
            }
        }
    }
    assert!(
        shared == alone,
        "view 0's sixth frame is the same with view 1 between its frames"
    );
}

/// A change of output size, ratio or backend starts a fresh history (`render.stats` says so).
#[test]
fn history_resets_on_resize_ratio_and_backend_change() {
    let Some(mut r) = renderer() else { return };
    let mut scene = fence_yard();
    let _ = converge(&mut r, &mut scene, own(Upscale::X1), 4);
    scene.render(&mut r, YARD.0, YARD.1);
    assert!(!r.stats().temporal.reset, "a running history");
    scene.render(&mut r, 320, 180);
    assert!(r.stats().temporal.reset, "a new size");
    scene.render(&mut r, 320, 180);
    assert!(!r.stats().temporal.reset);
    r.set_temporal(own(Upscale::X1_5));
    scene.render(&mut r, 320, 180);
    assert!(r.stats().temporal.reset, "a new ratio");
    r.set_temporal(Temporal::default());
    scene.render(&mut r, 320, 180);
    r.set_temporal(own(Upscale::X1_5));
    scene.render(&mut r, 320, 180);
    assert!(r.stats().temporal.reset, "temporal off then on");
}

/// Pixels that are infinite or NaN for one frame (a sun of infinite illuminance) leave no mark:
/// eight frames after, the image is within FLIP 0.01 of one that never had them.
#[test]
fn non_finite_input_does_not_poison_history() {
    let Some(mut r) = renderer() else { return };
    let mut scene = fence_yard();
    let clean = converge(&mut r, &mut scene, own(Upscale::X1), 24);
    let _ = converge(&mut r, &mut scene, own(Upscale::X1), 16);
    // The sun at infinite illuminance for one frame: every lit pixel is infinite or NaN.
    let rw = concordia_render::RenderWorld::extract(&scene.world);
    let sun = rw.lights.first().expect("the yard's sun").clone();
    let set = |scene: &mut Scene, lux: f32| {
        let light = concordia_render::DirectionalLight {
            illuminance_lux: lux,
            ..sun.light.clone()
        };
        scene.world.insert(sun.entity, light).unwrap();
    };
    set(&mut scene, f32::INFINITY);
    scene.render(&mut r, YARD.0, YARD.1);
    set(&mut scene, sun.light.illuminance_lux);
    let mut last = None;
    for _ in 0..8 {
        last = Some(scene.render(&mut r, YARD.0, YARD.1));
    }
    let score = flip(&clean, &last.unwrap());
    assert!(score.mean <= 0.01, "after a non-finite frame: {score:?}");
}

/// Debug views show this frame's raw data: with temporal on they render exactly as with it off.
#[test]
fn debug_views_render_without_temporal() {
    let Some(mut r) = renderer() else { return };
    let mut scene = fence_yard();
    let mode = concordia_render::ViewMode::normals;
    r.set_temporal(Temporal::default());
    let off = scene.render_mode(&mut r, YARD.0, YARD.1, mode);
    r.set_temporal(own(Upscale::X1_5));
    let on = scene.render_mode(&mut r, YARD.0, YARD.1, mode);
    assert!(on == off, "a debug view ignores temporal");
    assert_eq!(r.stats().temporal.backend, TemporalBackend::off);
}

/// `render.stats` reports what ran: the backend, the internal and output sizes and the phase.
#[test]
fn render_stats_report_the_temporal_backend_and_sizes() {
    let Some(mut r) = renderer() else { return };
    let mut scene = fence_yard();
    r.set_temporal(own(Upscale::X1_5));
    r.reset_motion_history();
    scene.render(&mut r, YARD.0, YARD.1);
    let first = r.stats().temporal;
    assert_eq!(first.backend, TemporalBackend::own);
    assert_eq!((first.internal, first.output), ([426, 240], [640, 360]));
    assert_eq!((first.phase, first.reset), (0, true));
    scene.render(&mut r, YARD.0, YARD.1);
    assert_eq!(
        (r.stats().temporal.phase, r.stats().temporal.reset),
        (1, false)
    );
    r.set_temporal(Temporal::default());
    scene.render(&mut r, YARD.0, YARD.1);
    assert_eq!(r.stats().temporal.backend, TemporalBackend::off);
}

/// Timings only, for the As built (4.6 plan Q13): the fence yard at 2560 × 1440 with temporal off
/// and the own resolve and MetalFX (4.6b) at 1.0×, 1.5× and 2.0×; each pass's median and the
/// frame's (MetalFX's own work shows only in the frame). Run on the
/// reference Mac:
/// `cargo nextest run -p concordia-render --release --run-ignored only -E 'test(fence_yard_temporal_timings)' --no-capture`.
#[test]
#[ignore = "timings for the As built, run by hand in release"]
fn fence_yard_temporal_timings() {
    let Some(mut r) = renderer() else { return };
    r.set_perf_mode(true);
    let mut s = fence_yard();
    let runs = [
        ("off", Temporal::default()),
        ("own 1.0×", own(Upscale::X1)),
        ("own 1.5×", own(Upscale::X1_5)),
        ("own 2.0×", own(Upscale::X2)),
        ("metalfx 1.0×", metalfx(Upscale::X1)),
        ("metalfx 1.5×", metalfx(Upscale::X1_5)),
        ("metalfx 2.0×", metalfx(Upscale::X2)),
    ];
    for (label, temporal) in runs {
        r.set_temporal(temporal);
        for (name, ms) in common::median_pass_times(&mut r, &mut s, (2560, 1440), 150) {
            println!("fence yard {label} {name}: median {ms:.3} ms");
        }
    }
}

fn metalfx(upscale: Upscale) -> Temporal {
    Temporal {
        backend: TemporalBackend::metalfx,
        upscale,
    }
}

/// Whether `r`'s device has MetalFX's temporal scaler (the MetalFX tests skip, saying why, if not).
fn has_metalfx(r: &Renderer) -> bool {
    let has = r.device().caps().metalfx_temporal;
    if !has {
        eprintln!("skipped: this device has no MetalFX temporal scaler (Apple GPUs only)");
    }
    has
}

/// On a device without MetalFX (masked), `metalfx` runs the own resolve: the stats say `own` and
/// the image is the own resolve's, byte for byte.
#[test]
fn metalfx_unavailable_falls_back_to_own() {
    let mask = concordia_rhi::CapsMask {
        metalfx_temporal: true,
        ..Default::default()
    };
    let Some(mut r) = common::renderer_masked(&mask) else {
        return;
    };
    let mut scene = fence_yard();
    let fallback = converge(&mut r, &mut scene, metalfx(Upscale::X1_5), 8);
    assert_eq!(r.stats().temporal.backend, TemporalBackend::own);
    let own = converge(&mut r, &mut scene, own(Upscale::X1_5), 8);
    assert!(fallback == own, "the fallback is the own resolve");
}

/// MetalFX at 1.5× (48 still frames) is within FLIP 0.06 of the own TAAU at 1.5×, and `render.stats`
/// says it ran at 1.5×.
#[test]
fn metalfx_backend_matches_own_taau_within_loose_tolerance() {
    let Some(mut r) = renderer() else { return };
    if !has_metalfx(&r) {
        return;
    }
    let mut scene = fence_yard();
    let theirs = converge(&mut r, &mut scene, metalfx(Upscale::X1_5), 48);
    let t = r.stats().temporal;
    assert_eq!(t.backend, TemporalBackend::metalfx);
    assert_eq!((t.internal, t.output), ([426, 240], [640, 360]));
    let ours = converge(&mut r, &mut scene, own(Upscale::X1_5), 48);
    let reference = supersampled(&mut r, &mut scene, YARD);
    review("fence_metalfx_1_5x", 0, &theirs);
    review("fence_own_1_5x", 0, &ours);
    let score = flip(&ours, &theirs);
    eprintln!(
        "MetalFX 1.5× against own {score:?}; against SSAA {:?} (own {:?})",
        flip(&reference, &theirs),
        flip(&reference, &ours)
    );
    assert!(
        score.mean <= 0.06,
        "MetalFX against the own TAAU: {score:?}"
    );
    goldens(&[("fence_metalfx_1_5x", &theirs)]);
}

/// MetalFX takes the jitter in its own sign and units: with the jitter held at an offset it pulls
/// the image back toward where an unjittered render puts it. It can only partly undo a constant
/// sub-pixel offset (0.45 px → 0.25 px measured), while a wrong sign pushes it further (0.65 px) and
/// a doubled scale overshoots (−0.56 px); the bound is 0.35 px.
#[test]
fn metalfx_still_scene_does_not_drift() {
    let Some(mut r) = renderer() else { return };
    if !has_metalfx(&r) {
        return;
    }
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
    let still = centroid(&scene.render(&mut r, 160, 90));
    r.set_temporal(metalfx(Upscale::X1));
    for offset in [[0.45, 0.0], [0.0, 0.45], [-0.3, 0.3]] {
        r.reset_motion_history();
        r.set_subpixel_override(Some(SubpixelOverride {
            offset,
            mip_bias: 0.0,
        }));
        let mut last = None;
        for _ in 0..8 {
            last = Some(scene.render(&mut r, 160, 90));
        }
        assert_eq!(r.stats().temporal.backend, TemporalBackend::metalfx);
        let at = centroid(&last.unwrap());
        for axis in 0..2 {
            let drift = at[axis] - still[axis];
            eprintln!("MetalFX offset {offset:?} axis {axis}: drift {drift:.3} px");
            assert!(
                drift.abs() <= 0.35,
                "offset {offset:?}, axis {axis}: drift {drift}"
            );
        }
    }
}

/// MetalFX keeps a panning image, pinning its motion vectors' sign and scale: FLIP ≤ 0.07 against
/// the supersampled pose (measured 0.061; the own TAA's 0.049; with a wrong sign 0.077–0.080, with
/// no motion 0.092).
#[test]
fn metalfx_panning_camera_converges() {
    let Some(mut r) = renderer() else { return };
    if !has_metalfx(&r) {
        return;
    }
    let score = pan(&mut r, metalfx(Upscale::X1), "panning_metalfx");
    eprintln!("MetalFX panning: {score:?}");
    assert!(
        score.mean <= 0.07,
        "MetalFX panning against SSAA: {score:?}"
    );
}

/// MetalFX takes the exposure record: one EV step, the next frame within 2 % of settled.
#[test]
fn metalfx_exposure_change_does_not_flash() {
    let Some(mut r) = renderer() else { return };
    if !has_metalfx(&r) {
        return;
    }
    let (a, b) = exposure_step(&mut r, metalfx(Upscale::X1));
    eprintln!("MetalFX exposure: next {a}, settled {b}");
    assert!(
        (a - b).abs() <= 0.02 * b,
        "the frame after the change: {a} against {b}"
    );
}

/// Switching own → MetalFX → off → own every 10 frames (40 in all) renders every frame, with the
/// stats naming the backend that ran and each switch starting a fresh history.
#[test]
fn upscaler_backends_are_interchangeable_at_runtime() {
    let Some(mut r) = renderer() else { return };
    let expected_metalfx = match r.device().caps().metalfx_temporal {
        true => TemporalBackend::metalfx,
        false => TemporalBackend::own,
    };
    let mut scene = fence_yard();
    let runs = [
        (own(Upscale::X1_5), TemporalBackend::own),
        (metalfx(Upscale::X1_5), expected_metalfx),
        (Temporal::default(), TemporalBackend::off),
        (own(Upscale::X1_5), TemporalBackend::own),
    ];
    for (i, (temporal, ran)) in runs.into_iter().enumerate() {
        r.set_temporal(temporal);
        for f in 0..10 {
            let image = scene.render(&mut r, YARD.0, YARD.1);
            let t = r.stats().temporal;
            assert_eq!(t.backend, ran, "run {i} frame {f}");
            assert_eq!(
                t.reset,
                f == 0 && ran != TemporalBackend::off,
                "run {i} frame {f}"
            );
            let lit = image.rgba.chunks(4).filter(|p| p[1] > 16).count();
            assert!(
                lit * 4 > image.rgba.len() / 4,
                "run {i} frame {f}: mostly black"
            );
        }
    }
}
