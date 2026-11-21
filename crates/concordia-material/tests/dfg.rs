//! Directional-albedo tables (M2 2.3a): the split-sum BRDF table moved here from the renderer,
//! and the sheen albedo table that keeps fuzz energy-conserving.

use concordia_material::dfg::{direct_albedo, integrate_sheen, sheen_albedo, specular_albedo};

#[test]
fn sheen_albedo_table_matches_brute_force_integration() {
    // (N·V, roughness, E) from an independent 400×400 midpoint integration in Python.
    for (nov, roughness, expected) in [
        (0.342_03, 0.6, 0.336_178),
        (0.3, 0.5, 0.321_4),
        (1.0, 1.0, 0.163_8),
        (0.7, 0.3, 0.028_8),
    ] {
        let brute = integrate_sheen(nov, roughness, 1 << 16);
        assert!(
            (brute - expected).abs() < 5e-3,
            "integral at ({nov}, {roughness}): {brute} vs {expected}"
        );
        let table = sheen_albedo(nov, roughness);
        assert!(
            (table - expected).abs() < 1e-2,
            "table at ({nov}, {roughness}): {table} vs {expected}"
        );
    }
}

#[test]
fn specular_albedo_reads_the_split_sum_table() {
    let [a, b] = specular_albedo(1.0, 0.0);
    assert!(
        (a + b - 1.0).abs() < 2e-2,
        "a smooth mirror head-on reflects F0 + (1 − F0)·0: {a} {b}"
    );
    let [a, b] = specular_albedo(0.5, 1.0);
    assert!(
        a + b < 0.8,
        "rough surfaces lose energy to single scattering: {a} {b}"
    );
}

#[test]
fn lookups_clamp_out_of_range_inputs() {
    assert!(
        sheen_albedo(0.0, 1.0) > 0.5,
        "rough sheen at grazing is bright"
    );
    assert_eq!(sheen_albedo(-1.0, 2.0), sheen_albedo(0.0, 1.0));
    assert!(specular_albedo(0.0, 0.5)[0] > 0.0);
    assert_eq!(specular_albedo(f32::NAN, 0.5), specular_albedo(0.0, 0.5));
}

#[test]
fn direct_albedo_matches_the_lobe_eval_slab_uses() {
    // E = ∫ F·D·V·N·L for F0 = 0.08 (eval_slab's GGX lobe), from a converged 800×1600 Python grid.
    for (mu, roughness, expected) in [
        (0.1f32, 0.6f32, 0.155_907f32),
        (0.1, 0.25, 0.432_0),
        (0.5, 0.6, 0.076_964),
        (1.0, 1.0, 0.024_579),
    ] {
        let [a, b] = direct_albedo(mu, roughness);
        let table = 0.08 * a + b;
        assert!(
            (table - expected).abs() < 5e-3,
            "at ({mu}, {roughness}): table {table} vs {expected}"
        );
    }
}
