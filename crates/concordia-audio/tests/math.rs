//! Gain, panning, distance and tick-frame arithmetic, every value derived by hand.
//!
//! - Decibels are 20·log₁₀ of an amplitude ratio, so a gain is 10^(dB/20): 0 dB → 1;
//!   20·log₁₀(0.5) = −6.0206 dB → 0.5; +6.0206 dB → 2. −80 dB and below is silence (0), and
//!   a gain of 0 reads back as −80 dB.
//! - Equal-power panning: θ = (p + 1)π/4, L = cos θ, R = sin θ. p = −1: θ = 0 → (1, 0);
//!   p = 0: θ = π/4 → (√½, √½) = (0.707107, 0.707107); p = 0.5: θ = 3π/8 → (0.382683, 0.923880);
//!   p = 1: θ = π/2 → (0, 1). L² + R² = cos² + sin² = 1 everywhere.
//! - Inverse distance, clamped: g = min / clamp(d, min, max). With min 1 and max 50: 0.5 m
//!   clamps to 1 → 1; 2 m → 0.5; 4 m → 0.25; 50 m and 100 m clamp to 50 → 0.02.
//! - Tick frames: [⌊t·48000/rate⌋, ⌊(t+1)·48000/rate⌋). At 60 Hz each tick is 800 frames, so
//!   tick 3 is [2400, 3200). At 50 Hz, 960 frames: tick 3 starts at 2880. At 144 Hz,
//!   48000/144 = 333.3̅: tick 2 is [⌊666.6̅⌋, ⌊1000⌋) = [666, 1000) (floored, not rounded to 667),
//!   and tick 3 is [1000, ⌊1333.3̅⌋) = [1000, 1333).

use std::num::NonZeroU32;

use concordia_audio::{
    Listener, SILENCE_DB, Spatial, db_to_gain, distance_gain, equal_power, gain_to_db,
    semitones_to_ratio, spatial_gains, tick_frames,
};
use concordia_core::math::{Quat, Vec3};
use std::f32::consts::FRAC_1_SQRT_2;

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-5, "{a} != {b}");
}

fn hz(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

#[test]
fn gain_and_decibels_convert_both_ways() {
    close(db_to_gain(0.0), 1.0);
    close(db_to_gain(-6.0206), 0.5);
    close(db_to_gain(6.0206), 2.0);
    assert_eq!(db_to_gain(SILENCE_DB), 0.0);
    assert_eq!(db_to_gain(-120.0), 0.0);
    assert_eq!(db_to_gain(f32::NAN), 0.0);
    close(gain_to_db(0.5), -6.0206);
    close(gain_to_db(1.0), 0.0);
    assert_eq!(gain_to_db(0.0), SILENCE_DB);
    assert_eq!(gain_to_db(f32::INFINITY), SILENCE_DB);
    // An octave doubles the rate: 2^(12/12) = 2; −12 halves it.
    close(semitones_to_ratio(12.0), 2.0);
    close(semitones_to_ratio(-12.0), 0.5);
    close(semitones_to_ratio(0.0), 1.0);
}

#[test]
fn equal_power_pan_follows_the_cos_sin_law() {
    let cases = [
        (-1.0, [1.0, 0.0]),
        (0.0, [FRAC_1_SQRT_2, FRAC_1_SQRT_2]),
        (0.5, [0.382683, 0.923880]),
        (1.0, [0.0, 1.0]),
    ];
    for (pan, [l, r]) in cases {
        let [gl, gr] = equal_power(pan);
        close(gl, l);
        close(gr, r);
        close(gl * gl + gr * gr, 1.0);
    }
    // Out-of-range pans clamp.
    assert_eq!(equal_power(3.0), equal_power(1.0));
}

#[test]
fn distance_attenuation_is_inverse_clamped() {
    let s = Spatial {
        min_distance: 1.0,
        max_distance: 50.0,
    };
    close(distance_gain(0.5, &s), 1.0);
    close(distance_gain(2.0, &s), 0.5);
    close(distance_gain(4.0, &s), 0.25);
    close(distance_gain(50.0, &s), 0.02);
    close(distance_gain(100.0, &s), 0.02);
    assert!(s.validate().is_ok());
    let bad = Spatial {
        min_distance: 2.0,
        max_distance: 1.0,
    };
    assert!(bad.validate().unwrap_err().contains("max_distance"));
}

/// A listener at the origin facing −z (right +x), and DC sources at min 1:
/// (2, 0, 0): d = 2 → 0.5, x = 2, z = 0 → p = 1 → (0, 0.5).
/// (0, 0, −4): d = 4 → 0.25, p = 0 → 0.25·0.707107 = 0.176777 each.
/// (−3, 0, 0): d = 3 → 1/3, p = −1 → (1/3, 0).
/// (2, 2, 0): above and to the right. The distance is 3D, √8 = 2.828427 → 0.353553, but the pan
/// uses the ground plane only (x = 2, z = 0 → p = 1), so it is fully right: (0, 0.353553).
/// Turned 90° about +y (counter-clockwise from above), its right is −z: (0, 0, −2) is then
/// fully right. With no listener, a source plays as if at the listener: centred, full gain.
#[test]
fn spatial_gains_follow_the_listener() {
    let s = Spatial {
        min_distance: 1.0,
        max_distance: 50.0,
    };
    let l = Listener::new(Vec3::ZERO, Quat::IDENTITY);
    let g = spatial_gains(Some(&l), Vec3::new(2.0, 0.0, 0.0), &s);
    close(g[0], 0.0);
    close(g[1], 0.5);
    let g = spatial_gains(Some(&l), Vec3::new(0.0, 0.0, -4.0), &s);
    close(g[0], 0.176777);
    close(g[1], 0.176777);
    let g = spatial_gains(Some(&l), Vec3::new(-3.0, 0.0, 0.0), &s);
    close(g[0], 1.0 / 3.0);
    close(g[1], 0.0);
    let g = spatial_gains(Some(&l), Vec3::new(2.0, 2.0, 0.0), &s);
    close(g[0], 0.0);
    close(g[1], 0.353_553);
    let turned = Listener::new(
        Vec3::ZERO,
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
    );
    let g = spatial_gains(Some(&turned), Vec3::new(0.0, 0.0, -2.0), &s);
    close(g[0], 0.0);
    close(g[1], 0.5);
    let g = spatial_gains(None, Vec3::new(9.0, 0.0, 0.0), &s);
    close(g[0], FRAC_1_SQRT_2);
    close(g[1], FRAC_1_SQRT_2);
    let g = spatial_gains(Some(&l), Vec3::ZERO, &s);
    close(g[0], FRAC_1_SQRT_2);
    close(g[1], FRAC_1_SQRT_2);
}

#[test]
fn commands_apply_at_their_tick_frame() {
    let rate = hz(48_000);
    assert_eq!(tick_frames(0, hz(60), rate), 0..800);
    assert_eq!(tick_frames(3, hz(60), rate), 2400..3200);
    assert_eq!(tick_frames(3, hz(50), rate), 2880..3840);
    assert_eq!(tick_frames(2, hz(144), rate), 666..1000);
    assert_eq!(tick_frames(3, hz(144), rate), 1000..1333);
    // A day of ticks at 60 Hz doesn't overflow or drift.
    let day = 60 * 60 * 60 * 24;
    assert_eq!(tick_frames(day, hz(60), rate).start, day * 800);
}
