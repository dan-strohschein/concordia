//! Image-based lighting on the CPU: equirect mapping, SH9 irradiance, GGX prefiltering and the
//! split-sum BRDF table (M1 phase 1.6).

use concordia_core::math::Vec3;
use concordia_render::ibl::{
    BRDF_LUT_SAMPLES, BRDF_LUT_SIZE, Equirect, brdf_lut, dir_to_uv, eval_sh9, integrate_brdf,
    prefilter, sh9_irradiance, uv_to_dir,
};

fn uniform(value: [f32; 3]) -> Equirect {
    Equirect::new(64, 32, vec![value; 64 * 32]).unwrap()
}

#[test]
fn equirect_mapping_round_trips() {
    for d in [
        Vec3::X,
        Vec3::NEG_Z,
        Vec3::new(0.3, 0.8, -0.5).normalize(),
        Vec3::new(-0.7, -0.2, 0.4).normalize(),
    ] {
        let back = uv_to_dir(dir_to_uv(d));
        assert!(back.abs_diff_eq(d, 1e-5), "{d} → {back}");
    }
    assert!((dir_to_uv(Vec3::Y)[1]).abs() < 1e-6, "up is the top row");
}

#[test]
fn irradiance_sh9_of_uniform_environment_is_constant() {
    let sh = sh9_irradiance(&uniform([0.5, 1.0, 2.0]));
    for (i, c) in sh.iter().enumerate().skip(1) {
        assert!(
            c.iter().all(|v| v.abs() < 1e-4),
            "band term {i} is zero: {c:?}"
        );
    }
    for n in [Vec3::Y, Vec3::NEG_X, Vec3::new(0.2, -0.9, 0.3).normalize()] {
        let e = eval_sh9(&sh, n);
        let expected = [0.5, 1.0, 2.0].map(|l| l * std::f32::consts::PI);
        assert!(
            e.iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 1e-3 * b),
            "irradiance of uniform L is πL: {e:?}"
        );
    }
}

#[test]
fn sh9_irradiance_is_brighter_facing_a_bright_sky() {
    let mut map = uniform([0.1; 3]);
    for y in 0..8 {
        for x in 0..64 {
            map.set(x, y, [5.0; 3]);
        }
    }
    let sh = sh9_irradiance(&map);
    assert!(eval_sh9(&sh, Vec3::Y)[0] > 4.0 * eval_sh9(&sh, Vec3::NEG_Y)[0]);
}

#[test]
fn prefiltered_mip0_equals_source_for_roughness_zero() {
    let mut map = uniform([0.2; 3]);
    map.set(10, 12, [9.0, 3.0, 1.0]);
    let levels = prefilter(&map, 4, 64);
    assert_eq!(levels.len(), 4);
    assert_eq!(levels[0], map, "roughness 0 is the source itself");
    assert!(
        levels[3].width() <= levels[1].width(),
        "rougher levels are no larger"
    );
}

#[test]
fn prefiltering_a_uniform_environment_keeps_it_uniform() {
    let levels = prefilter(&uniform([0.7, 0.7, 0.7]), 5, 64);
    for level in &levels {
        assert!(
            level.pixels().iter().all(|p| (p[0] - 0.7).abs() < 1e-3),
            "energy is conserved"
        );
    }
}

#[test]
fn brdf_lut_matches_reference_values() {
    let size = BRDF_LUT_SIZE;
    let lut = brdf_lut(size, BRDF_LUT_SAMPLES);
    for (nov_i, rough_i) in [(31, 0), (16, 16), (4, 28), (28, 4), (8, 8)] {
        let (nov, rough) = (
            (nov_i as f32 + 0.5) / size as f32,
            (rough_i as f32 + 0.5) / size as f32,
        );
        let reference = integrate_brdf(nov, rough, 16384);
        let got = lut[rough_i * size + nov_i];
        assert!(
            (got[0] - reference[0]).abs() < 1e-3 && (got[1] - reference[1]).abs() < 1e-3,
            "({nov}, {rough}): {got:?} vs {reference:?}"
        );
    }
    let smooth_head_on = integrate_brdf(1.0, 0.0, 4096);
    assert!(
        (smooth_head_on[0] + smooth_head_on[1] - 1.0).abs() < 1e-2,
        "a mirror reflects everything head-on: {smooth_head_on:?}"
    );
}

#[test]
fn hdr_fixture_decodes_to_an_equirect() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/hdri/kloofendal_48d_partly_cloudy_puresky_512.hdr"
    ))
    .unwrap();
    let texture =
        concordia_assets::decode_image(&bytes, &concordia_assets::ImageSettings::default())
            .unwrap();
    let map = Equirect::from_texture(&texture).unwrap();
    assert_eq!((map.width(), map.height()), (512, 256));
    let sky = map.get(256, 20);
    let ground = map.get(256, 240);
    assert!(
        sky.iter().sum::<f32>() > ground.iter().sum::<f32>(),
        "a pure sky is brighter above: {sky:?} vs {ground:?}"
    );
}
