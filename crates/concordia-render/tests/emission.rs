//! Emission is luminance in nits: exposure scales it like any light (ADR-0102, M4 4.1b4). Unlit
//! materials stay screen-referred. GPU cases skip without a GPU unless CONCORDIA_REQUIRE_GPU=1.

mod common;

use common::{M1, Scene, pixel, renderer};
use concordia_core::math::{Quat, Vec3};
use concordia_render::{Camera, Projection, Tonemap};

/// A glowing (black, unlit = `unlit`) plane filling an orthographic view at `ev100`, Linear
/// tone mapping, no lights; the centre pixel in 0..=1.
fn glow(ev100: f32, nits: f32, unlit: bool) -> f32 {
    let Some(mut r) = renderer() else {
        return f32::NAN;
    };
    let mut s = Scene::new();
    let m = s.material(
        "glow",
        M1 {
            base_color: [0.0, 0.0, 0.0, 1.0],
            emissive: [1.0, 1.0, 1.0],
            emissive_strength: nits,
            unlit,
            ..M1::default()
        },
    );
    s.mesh("plane", m, Vec3::ZERO, Quat::IDENTITY, Vec3::splat(10.0));
    s.camera(
        Vec3::new(0.0, 5.0, 0.01),
        Vec3::ZERO,
        Camera {
            projection: Projection::Orthographic {
                height: 2.0,
                near: 0.1,
                far: 20.0,
            },
            exposure_ev100: ev100,
            tonemap: Tonemap::Linear,
            ..Camera::default()
        },
    );
    let image = s.render(&mut r, 8, 8);
    f32::from(pixel(&image, 4, 4)[0]) / 255.0
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[test]
fn emission_is_luminance_scaled_by_exposure() {
    // 0.6 nits at EV 0 (exposure 1/1.2) shows 0.5; one stop brighter exposure... EV 1 halves it.
    let at = |ev: f32| glow(ev, 0.6, false);
    let (ev0, ev1) = (at(0.0), at(1.0));
    if ev0.is_nan() {
        return;
    }
    assert!(
        (srgb_to_linear(ev0) - 0.5).abs() < 0.01,
        "0.6 nits × 1/1.2 = 0.5, got {}",
        srgb_to_linear(ev0)
    );
    assert!(
        (srgb_to_linear(ev1) - 0.25).abs() < 0.01,
        "EV 1 halves it, got {}",
        srgb_to_linear(ev1)
    );
}

#[test]
fn unlit_emission_stays_screen_referred() {
    let (ev0, ev3) = (glow(0.0, 0.5, true), glow(3.0, 0.5, true));
    if ev0.is_nan() {
        return;
    }
    assert!(
        (srgb_to_linear(ev0) - 0.5).abs() < 0.01,
        "{}",
        srgb_to_linear(ev0)
    );
    assert_eq!(ev0, ev3, "exposure doesn't touch unlit surfaces");
}
