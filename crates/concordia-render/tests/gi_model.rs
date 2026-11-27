//! The probe GI's CPU model (M4 4.8b): the math the GPU update and sampling use, tested on its own.

use concordia_core::math::{UVec2, Vec2, Vec3};
use concordia_render::gi::{self, ProbeRay, ProbeState};
use proptest::prelude::*;

fn dir() -> impl Strategy<Value = Vec3> {
    (-1.0f32..1.0, -1.0f32..1.0, -1.0f32..1.0)
        .prop_filter("not zero", |(x, y, z)| x * x + y * y + z * z > 1e-4)
        .prop_map(|(x, y, z)| Vec3::new(x, y, z).normalize())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn probe_octahedral_encoding_roundtrips(d in dir()) {
        let p = gi::octa::encode(d);
        prop_assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0, "{p}");
        let back = gi::octa::decode(p);
        prop_assert!((back - d).length() < 1e-5, "{d} → {p} → {back}");
    }
}

#[test]
fn every_octahedral_texel_centre_is_a_unit_direction_that_maps_back_to_it() {
    for size in [8u32, 16] {
        for y in 0..size {
            for x in 0..size {
                let t = UVec2::new(x, y);
                let d = gi::octa::texel_dir(t, size);
                assert!((d.length() - 1.0).abs() < 1e-5);
                assert_eq!(gi::octa::texel_of(d, size), t, "size {size}");
            }
        }
    }
}

#[test]
fn encode_never_panics_on_a_zero_direction() {
    assert!(gi::octa::encode(Vec3::ZERO).is_finite());
    assert!(gi::octa::encode(Vec3::NAN).is_finite());
}

#[test]
fn chebyshev_visibility_rejects_probe_behind_wall() {
    // A probe 1 m from a wall: rays toward the shaded point stop at the wall (mean 1, little
    // spread). A point 3 m away behind the wall is nearly invisible; one in front is fully seen.
    let moments = Vec2::new(1.0, 1.0 + 0.01 * 0.01);
    assert!(gi::chebyshev(moments, 3.0) < 0.01, "behind the wall");
    assert_eq!(gi::chebyshev(moments, 0.9), 1.0, "in front of the wall");
    // Open space: rays went far, a point at 3 m is visible.
    let open = Vec2::new(8.0, 64.5);
    assert!(gi::chebyshev(open, 3.0) > 0.99);
}

#[test]
fn chebyshev_is_bounded_on_any_input() {
    for m in [
        Vec2::ZERO,
        Vec2::new(1.0, 0.0),
        Vec2::new(-1.0, 5.0),
        Vec2::NAN,
    ] {
        for d in [0.0, 0.5, 10.0, f32::INFINITY, f32::NAN] {
            let w = gi::chebyshev(m, d);
            assert!((0.0..=1.0).contains(&w), "{m} {d}: {w}");
        }
    }
}

/// Rays from a probe: `n` evenly spread directions hitting at `dist`, a fraction `back` of them
/// backfaces (inside geometry).
fn rays(n: usize, dist: impl Fn(Vec3) -> Option<(f32, bool)>) -> Vec<ProbeRay> {
    (0..n)
        .map(|i| {
            // Spherical Fibonacci.
            let z = 1.0 - (2.0 * i as f32 + 1.0) / n as f32;
            let r = (1.0 - z * z).sqrt();
            let a = i as f32 * 2.399_963;
            let d = Vec3::new(r * a.cos(), z, r * a.sin());
            match dist(d) {
                Some((t, backface)) => ProbeRay {
                    dir: d,
                    hit: Some(t),
                    backface,
                },
                None => ProbeRay {
                    dir: d,
                    hit: None,
                    backface: false,
                },
            }
        })
        .collect()
}

#[test]
fn probe_relocation_moves_probes_out_of_geometry() {
    let spacing = 1.5;
    // A probe 0.2 m inside a slab whose top face is above it (+Y): rays going up hit the slab's
    // inside (backfaces) at 0.2 m; the rest hit its other inside faces far away, also backfaces,
    // except the floor of the slab's open bottom... keep it simple: everything below the probe is
    // open, everything above is the slab's inside.
    let inside = rays(128, |d| (d.y > 0.0).then(|| (0.2 / d.y.max(1e-3), true)));
    let (offset, state) = gi::relocate(&inside, Vec3::ZERO, spacing);
    assert!(
        offset.y > 0.2,
        "moves up through the face it is behind: {offset}"
    );
    assert!(offset.length() <= 0.45 * spacing + 1e-6, "{offset}");
    assert_eq!(state, ProbeState::Active);

    // In open space nothing moves.
    let open = rays(128, |_| None);
    let (offset, _) = gi::relocate(&open, Vec3::ZERO, spacing);
    assert_eq!(offset, Vec3::ZERO);

    // Too close to a wall (0.05 m, frontfaces): moved away from it, not through it.
    let near_wall = rays(128, |d| (d.x > 0.0).then(|| (0.05 / d.x.max(1e-3), false)));
    let (offset, _) = gi::relocate(&near_wall, Vec3::ZERO, spacing);
    assert!(offset.x < 0.0, "away from the wall: {offset}");

    // Buried: every ray hits a backface → inactive.
    let buried = rays(128, |_| Some((0.3, true)));
    let (_, state) = gi::relocate(&buried, Vec3::ZERO, spacing);
    assert_eq!(state, ProbeState::Inactive);

    // Nothing within reach at all (open sky over nothing): inactive too (it lights nothing).
    let (_, state) = gi::relocate(&open, Vec3::ZERO, spacing);
    assert_eq!(state, ProbeState::Inactive);
}

#[test]
fn relocation_keeps_its_previous_offset_as_a_start() {
    let spacing = 1.5;
    let inside = rays(128, |d| (d.y > 0.0).then(|| (0.2 / d.y.max(1e-3), true)));
    let (first, _) = gi::relocate(&inside, Vec3::ZERO, spacing);
    let (second, _) = gi::relocate(&inside, first, spacing);
    assert!(second.length() <= 0.45 * spacing + 1e-6);
    assert!(second.y >= first.y, "{first} then {second}");
}

/// The irradiance after `frames` of blending samples from `sample(frame)` (starting dark), with a
/// light event on `event` frames.
fn run(frames: usize, sample: impl Fn(usize) -> f32, event: impl Fn(usize) -> bool) -> Vec<f32> {
    let mut value = 0.0;
    (0..frames)
        .map(|f| {
            let s = sample(f);
            let h = gi::hysteresis(value, s, gi::BASE_HYSTERESIS, event(f));
            value = value * h + s * (1.0 - h);
            value
        })
        .collect()
}

#[test]
fn adaptive_hysteresis_reacts_within_4_frames_to_spell_light() {
    // Dark until frame 10, when a spell light spawns: samples jump to 1 (± small noise).
    let noise = |f: usize| 0.02 * ((f as f32 * 12.9898).sin() * 43_758.547).fract();
    let lit = run(
        40,
        |f| if f < 10 { 0.0 } else { 1.0 + noise(f) },
        |f| f == 10,
    );
    assert!(lit[13] >= 0.9, "90% within 4 frames: {:?}", &lit[9..15]);
    // Without the light event it still adapts (large change), within 6 frames.
    let quiet = run(40, |f| if f < 10 { 0.0 } else { 1.0 }, |_| false);
    assert!(quiet[15] >= 0.9, "{:?}", &quiet[9..17]);
}

#[test]
fn hysteresis_keeps_a_static_scene_stable_to_one_percent() {
    // Converged at 1.0, then 200 frames of ±5% sampling noise: the value stays within ±1%.
    let mut value = 1.0f32;
    let mut worst = 0.0f32;
    for f in 0..200 {
        let s = 1.0 + 0.05 * (2.0 * ((f as f32 * 78.233).sin() * 43_758.547).fract().abs() - 1.0);
        let h = gi::hysteresis(value, s, gi::BASE_HYSTERESIS, false);
        value = value * h + s * (1.0 - h);
        worst = worst.max((value - 1.0).abs());
    }
    assert!(worst <= 0.01, "drifted {worst}");
}

#[test]
fn hysteresis_is_a_weight_in_0_1_on_any_input() {
    for (prev, s) in [
        (0.0, 0.0),
        (1.0, f32::NAN),
        (f32::INFINITY, 1.0),
        (-1.0, 2.0),
    ] {
        for event in [false, true] {
            let h = gi::hysteresis(prev, s, 0.97, event);
            assert!((0.0..=1.0).contains(&h), "{prev} {s}: {h}");
        }
    }
}
