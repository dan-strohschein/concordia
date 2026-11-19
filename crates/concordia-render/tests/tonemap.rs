//! Tonemapping CPU references (M1 phase 1.6); the GPU must match these (see render.rs).

use concordia_render::Tonemap;
use concordia_render::tonemap::{apply, srgb_encode};

fn srgb8(linear: f32) -> u8 {
    (srgb_encode(linear) * 255.0).round() as u8
}

#[test]
fn tonemap_agx_maps_mid_grey_to_expected_srgb() {
    let [r, g, b] = apply(Tonemap::AgX, [0.18; 3]);
    assert_eq!(
        [srgb8(r), srgb8(g), srgb8(b)],
        [128, 128, 128],
        "AgX puts 18% grey at display mid-grey"
    );
    let [white, ..] = apply(Tonemap::AgX, [1.0; 3]);
    assert_eq!(srgb8(white), 202, "diffuse white keeps headroom");
}

#[test]
fn agx_compresses_highlights_monotonically() {
    let values: Vec<f32> = [0.01, 0.1, 0.5, 1.0, 4.0, 16.0, 64.0]
        .iter()
        .map(|v| apply(Tonemap::AgX, [*v; 3])[0])
        .collect();
    assert!(values.windows(2).all(|w| w[1] > w[0]), "{values:?}");
    assert!(values.iter().all(|v| (0.0..=1.0).contains(v)), "{values:?}");
}

#[test]
fn khronos_pbr_neutral_is_an_offset_identity_below_compression_start() {
    // Spec: below the compression start (0.76 after the offset) colors only lose the toe offset (0.04).
    let c = [0.5, 0.3, 0.2];
    let out = apply(Tonemap::KhronosPbrNeutral, c);
    for i in 0..3 {
        assert!((out[i] - (c[i] - 0.04)).abs() < 1e-6, "{out:?}");
    }
    let bright = apply(Tonemap::KhronosPbrNeutral, [8.0, 2.0, 1.0]);
    assert!(
        bright.iter().all(|v| *v <= 1.0) && bright[0] > bright[1] && bright[1] > bright[2],
        "{bright:?}"
    );
}

#[test]
fn aces_fitted_follows_the_narkowicz_curve() {
    let x = 0.5f32;
    let expected = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
    assert!((apply(Tonemap::AcesFitted, [x; 3])[0] - expected).abs() < 1e-6);
    assert!(apply(Tonemap::AcesFitted, [100.0; 3])[0] <= 1.0);
}

#[test]
fn linear_clamps() {
    assert_eq!(apply(Tonemap::Linear, [0.25, 1.5, -1.0]), [0.25, 1.0, 0.0]);
}

#[test]
fn srgb_encode_matches_the_standard_curve() {
    assert_eq!(srgb8(0.0), 0);
    assert_eq!(srgb8(1.0), 255);
    assert_eq!(srgb8(0.2159), 128, "sRGB 128 is linear 0.2159");
    assert!(
        (srgb_encode(0.002) - 0.002 * 12.92).abs() < 1e-7,
        "linear toe"
    );
}

#[test]
fn default_tonemap_is_agx() {
    assert_eq!(Tonemap::default(), Tonemap::AgX);
}
