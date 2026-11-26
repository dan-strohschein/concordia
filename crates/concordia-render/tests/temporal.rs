//! Temporal anti-aliasing and upscaling (M4 4.6): the camera's sub-pixel jitter and the texture mip
//! bias, then (4.6a steps 3–5) the own TAA/TAAU against supersampled references.

mod common;
use common::{Scene, renderer, unlit};
use concordia_assets::{MeshRenderer, TextureData, TextureFormat};
use concordia_core::math::{Quat, Vec3};
use concordia_render::temporal::SubpixelOverride;
use concordia_render::{Camera, Image, Msaa, Projection};

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
