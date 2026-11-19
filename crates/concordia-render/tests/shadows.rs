//! Cascaded shadows on the GPU (M1 phase 1.6). Skips without a GPU unless CONCORDIA_REQUIRE_GPU=1.

mod common;

use common::{Scene, golden, pixel, renderer};
use concordia_assets::MaterialData;
use concordia_core::math::{EulerRot, Quat, Vec3};
use concordia_render::shadow::CascadeSettings;
use concordia_render::{Camera, DirectionalLight, Image, Msaa, Projection, Renderer, Tonemap};

fn iso() -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        std::f32::consts::FRAC_PI_4,
        -35.264f32.to_radians(),
        0.0,
    )
}

/// A grey floor, optionally a box, a sun along `sun` (with or without shadows) and an iso camera centered on `look`.
fn scene(
    with_box: bool,
    sun: Vec3,
    shadows: Option<CascadeSettings>,
    look: Vec3,
    msaa: Msaa,
) -> Scene {
    let mut s = Scene::new();
    let grey = s.material(
        "grey",
        MaterialData {
            base_color: [0.7, 0.7, 0.7, 1.0],
            metallic: 0.0,
            roughness: 0.9,
            ..MaterialData::default()
        },
    );
    s.mesh(
        "plane",
        grey.clone(),
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::splat(60.0),
    );
    if with_box {
        s.mesh(
            "cube",
            grey,
            Vec3::new(0.0, 1.0, 0.0),
            Quat::from_rotation_y(0.4),
            Vec3::new(2.0, 2.0, 2.0),
        );
    }
    let id = s.entity(
        Vec3::ZERO,
        Quat::from_rotation_arc(Vec3::NEG_Z, sun.normalize()),
        Vec3::ONE,
    );
    s.world
        .insert(
            id,
            DirectionalLight {
                color: [1.0; 3],
                illuminance_lux: 100_000.0,
                shadows,
            },
        )
        .unwrap();
    let camera = Camera {
        projection: Projection::Orthographic {
            height: 16.0,
            near: 0.1,
            far: 100.0,
        },
        msaa,
        tonemap: Tonemap::Linear,
        ..Camera::default()
    };
    let id = s.entity(look + iso() * Vec3::new(0.0, 0.0, 40.0), iso(), Vec3::ONE);
    s.world.insert(id, camera).unwrap();
    s
}

fn shadows() -> Option<CascadeSettings> {
    Some(CascadeSettings {
        cascades: 3,
        resolution: 1024,
        ..CascadeSettings::default()
    })
}

#[test]
fn box_on_plane_casts_shadow_golden() {
    let Some(mut r) = renderer() else { return };
    // Light from the camera's left: one visible face lit, the shadow falls across the screen.
    let sun = Vec3::new(0.7, -1.0, -0.7);
    let lit = scene(true, sun, None, Vec3::ZERO, Msaa::X4).render(&mut r, 384, 256);
    let shadowed = scene(true, sun, shadows(), Vec3::ZERO, Msaa::X4).render(&mut r, 384, 256);
    let darker = lit
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(shadowed.rgba.as_chunks::<4>().0)
        .filter(|(a, b)| b[0] + 20 < a[0])
        .count();
    assert!(
        darker > 500,
        "the box shadows part of the floor: {darker} pixels"
    );
    golden("shadow_box_plane_iso", &shadowed, 0.01);
}

#[test]
fn no_shadow_acne_on_flat_plane_at_grazing_sun() {
    let Some(mut r) = renderer() else { return };
    let elevation = 10f32.to_radians();
    let sun = Vec3::new(-elevation.cos(), -elevation.sin(), 0.0);
    let reference = scene(false, sun, None, Vec3::ZERO, Msaa::Off).render(&mut r, 256, 256);
    let shadowed = scene(false, sun, shadows(), Vec3::ZERO, Msaa::Off).render(&mut r, 256, 256);
    let acne = reference
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(shadowed.rgba.as_chunks::<4>().0)
        .filter(|(a, b)| b[0] + 2 < a[0])
        .count();
    assert!(
        acne * 1000 < 256 * 256,
        "{acne} pixels darker than the unshadowed plane"
    );
}

/// Worst count, over 30 frames, of pixels that differ by more than one level from the previous
/// frame shifted by one pixel, while the iso camera pans exactly one screen pixel per frame.
fn shimmer(r: &mut Renderer, stabilize: bool) -> usize {
    let (w, h) = (256u32, 256u32);
    let pixel_world = 16.0 / h as f32;
    let right = iso() * Vec3::X;
    let sun = Vec3::new(0.7, -0.6, -0.7);
    // Coarse maps make each shadow texel span several pixels, so unsnapped cascades visibly crawl.
    let settings = Some(CascadeSettings {
        cascades: 1,
        resolution: 256,
        stabilize,
        ..CascadeSettings::default()
    });
    let frame = |r: &mut Renderer, i: u32| {
        scene(
            true,
            sun,
            settings.clone(),
            right * pixel_world * i as f32,
            Msaa::Off,
        )
        .render(r, w, h)
    };
    let mut previous = frame(r, 0);
    let mut worst = 0usize;
    for i in 1..30 {
        let current = frame(r, i);
        worst = worst.max(shifted_differences(&previous, &current));
        previous = current;
    }
    worst
}

#[test]
fn shadow_shimmer_is_below_threshold_while_panning() {
    let Some(mut r) = renderer() else { return };
    let (stable, unstable) = (shimmer(&mut r, true), shimmer(&mut r, false));
    assert!(
        stable < 20,
        "stabilized cascades: {stable} pixels changed beyond a one-pixel shift"
    );
    assert!(
        unstable > 5 * stable.max(10),
        "the measure detects shimmer: unstabilized {unstable} vs stabilized {stable}"
    );
}

/// Pixels (away from the borders) whose red channel differs by more than 1 between `a` and `b`
/// shifted left by one pixel.
fn shifted_differences(a: &Image, b: &Image) -> usize {
    let mut n = 0;
    for y in 4..a.height - 4 {
        for x in 4..a.width - 4 {
            if pixel(a, x, y)[0].abs_diff(pixel(b, x - 1, y)[0]) > 1 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn unchanged_shadows_are_reused_and_moving_a_caster_redraws_them() {
    let Some(mut r) = renderer() else { return };
    let sun = Vec3::new(0.7, -1.0, -0.7);
    let mut s = scene(true, sun, shadows(), Vec3::ZERO, Msaa::X4);
    let first = s.render(&mut r, 192, 128);
    assert!(
        !r.stats().shadows_cached,
        "the first frame draws its shadows"
    );
    let again = s.render(&mut r, 192, 128);
    assert!(
        r.stats().shadows_cached,
        "nothing that casts or receives changed"
    );
    assert_eq!(first.rgba, again.rgba, "reused shadows look the same");
    let boxes: Vec<_> = s.world.entity_ids_sorted();
    let cube = boxes
        .into_iter()
        .find(|id| {
            s.world
                .get::<concordia_scene::builtin::Transform>(*id)
                .is_some_and(|t| t.translation.y == 1.0)
        })
        .unwrap();
    s.world
        .get_mut::<concordia_scene::builtin::Transform>(cube)
        .unwrap()
        .translation
        .x = 3.0;
    let moved = s.render(&mut r, 192, 128);
    assert!(
        !r.stats().shadows_cached,
        "a moved caster redraws the shadows"
    );
    assert_ne!(first.rgba, moved.rgba);
}
