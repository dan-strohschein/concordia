//! `slab_bsdf_features_match_reference_values`, one test per feature (M2 2.3a, ADR-0068). The
//! expected numbers come from an independent Python implementation of the ADR's formulas. Diffuse
//! is scaled by 1 − E_spec(N·V) (energy-conserving), read from a table, hence the looser tolerance.

use concordia_assets::{Fuzz, Haze, Slab, Subsurface, ThinFilm};
use concordia_core::math::Vec3;
use concordia_material::{eval_slab, thin_film_reflectance};

const N: Vec3 = Vec3::Z;

/// A unit direction `theta` degrees from the normal, at azimuth `phi` degrees.
fn dir(theta: f32, phi: f32) -> Vec3 {
    let (t, p) = (theta.to_radians(), phi.to_radians());
    Vec3::new(t.sin() * p.cos(), t.sin() * p.sin(), t.cos())
}

fn assert_close(actual: Vec3, expected: [f32; 3], relative: f32) {
    for (i, (a, e)) in actual.to_array().into_iter().zip(expected).enumerate() {
        let tolerance = relative * e.abs().max(1e-3);
        assert!(
            (a - e).abs() <= tolerance,
            "channel {i}: {a} vs {e} (±{tolerance}); {actual:?}"
        );
    }
}

fn slab(base: [f32; 3], metallic: f32, roughness: f32) -> Slab {
    Slab {
        base_color: [base[0], base[1], base[2], 1.0],
        metallic,
        roughness,
        ..Slab::default()
    }
}

const GOLD: [f32; 3] = [1.0, 0.766, 0.336];

#[test]
fn slab_bsdf_diffuse_matches_reference_values() {
    let s = slab([0.5, 0.25, 0.125], 0.0, 0.6);
    let f = eval_slab(&s, N, dir(0.0, 0.0), dir(60.0, 0.0));
    assert_close(f, [0.080_094_9, 0.041_619_946, 0.022_382_467], 2e-3);
}

#[test]
fn slab_bsdf_specular_matches_reference_values() {
    let f = eval_slab(&slab(GOLD, 1.0, 0.4), N, dir(0.0, 0.0), dir(30.0, 0.0));
    assert_close(f, [0.246_171_66, 0.188_567_49, 0.082_713_684], 1e-4);
}

#[test]
fn slab_bsdf_haze_matches_reference_values() {
    let s = Slab {
        haze: Some(Haze {
            roughness: 0.8,
            weight: 0.3,
        }),
        ..slab(GOLD, 1.0, 0.4)
    };
    let f = eval_slab(&s, N, dir(0.0, 0.0), dir(30.0, 0.0));
    assert_close(f, [0.219_241_3, 0.167_938_84, 0.073_665_08], 1e-4);
}

#[test]
fn slab_bsdf_fuzz_matches_reference_values() {
    let s = Slab {
        fuzz: Some(Fuzz {
            color: [0.6, 0.5, 0.4],
            roughness: 0.6,
        }),
        ..slab([0.5; 3], 0.0, 0.5)
    };
    // Sheen lobe plus the base scaled by 1 − max(color) × E(N·V); E comes from a table.
    let f = eval_slab(&s, N, dir(70.0, 0.0), dir(60.0, 0.0));
    assert_close(f, [0.119_721_11, 0.109_549_29, 0.099_377_48], 2e-3);
}

#[test]
fn slab_bsdf_subsurface_matches_reference_values() {
    let base = slab([0.8, 0.4, 0.3], 0.0, 0.5);
    let s = Slab {
        subsurface: Some(Subsurface {
            mean_free_path: [0.01, 0.004, 0.002],
        }),
        ..base.clone()
    };
    // Light 10° below the horizon: only wrapped diffuse reaches the eye.
    let (v, l) = (dir(0.0, 0.0), dir(100.0, 0.0));
    assert_close(
        eval_slab(&s, N, v, l),
        [0.035_581_47, 0.008_315_258, 0.0],
        2e-3,
    );
    assert_eq!(
        eval_slab(&base, N, v, l),
        Vec3::ZERO,
        "no subsurface, no light from behind"
    );
}

#[test]
fn slab_bsdf_thin_film_matches_reference_values() {
    let f0 = Vec3::splat(0.04);
    let n1 = 1.5f32.sqrt();
    let quarter_wave = ThinFilm {
        thickness_nm: 532.0 / (4.0 * n1),
        ior: n1,
    };
    let r = thin_film_reflectance(f0, 1.0, &quarter_wave);
    assert!(
        r.y.abs() < 1e-6,
        "a quarter-wave film cancels its design wavelength: {r:?}"
    );
    assert_close(r, [0.002_432_66, 0.0, 0.002_093_792], 1e-3);
    let none = ThinFilm {
        thickness_nm: 0.0,
        ior: n1,
    };
    assert_close(
        thin_film_reflectance(f0, 0.5, &none),
        [0.089_186_71; 3],
        1e-4,
    );
    let soap = ThinFilm {
        thickness_nm: 300.0,
        ior: 1.33,
    };
    let cos45 = std::f32::consts::FRAC_1_SQRT_2;
    assert_close(
        thin_film_reflectance(f0, cos45, &soap),
        [0.048_370_35, 0.029_036_223, 0.012_397_291],
        1e-4,
    );
}

#[test]
fn a_thin_film_replaces_the_slab_fresnel() {
    let n1 = 1.5f32.sqrt();
    let film = ThinFilm {
        thickness_nm: 532.0 / (4.0 * n1),
        ior: n1,
    };
    let plain = slab([0.0; 3], 0.0, 0.3);
    let filmed = Slab {
        thin_film: Some(film),
        ..plain.clone()
    };
    let (v, l) = (dir(0.0, 0.0), dir(0.0, 0.0));
    let (a, b) = (eval_slab(&plain, N, v, l), eval_slab(&filmed, N, v, l));
    assert!(
        b.y < a.y * 1e-3,
        "head-on, the film cancels green: {a:?} vs {b:?}"
    );
    assert!(b.x > 0.0 && b.z > 0.0);
}

#[test]
fn directions_need_not_be_normalized_and_bad_input_gives_zero() {
    let s = slab(GOLD, 1.0, 0.4);
    let unit = eval_slab(&s, N, dir(0.0, 0.0), dir(30.0, 0.0));
    assert!(
        unit.min_element() > 0.05,
        "gold reflects every channel: {unit:?}"
    );
    let scaled = eval_slab(&s, N * 3.0, dir(0.0, 0.0) * 2.0, dir(30.0, 0.0) * 0.5);
    assert!((unit - scaled).abs().max_element() < 1e-6);
    assert_eq!(eval_slab(&s, N, Vec3::ZERO, dir(30.0, 0.0)), Vec3::ZERO);
    assert_eq!(eval_slab(&s, N, Vec3::NAN, dir(30.0, 0.0)), Vec3::ZERO);
}
