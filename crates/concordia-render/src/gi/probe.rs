//! Per-probe rules: visibility, relocation and the blend weight.

use concordia_core::math::{Vec2, Vec3};

/// The blend weight of the old value when nothing changes (Majercik et al. 2019).
pub const BASE_HYSTERESIS: f32 = 0.97;

/// One of a probe's rays this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProbeRay {
    /// Unit direction.
    pub dir: Vec3,
    /// Hit distance (`None`: a miss).
    pub hit: Option<f32>,
    /// The hit is a backface (the probe is inside geometry along this ray).
    pub backface: bool,
}

/// Whether a probe is traced and sampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeState {
    /// Traced and sampled.
    Active,
    /// Buried in geometry or seeing nothing: skipped.
    Inactive,
}

/// How visible a point at `distance` is from a probe whose rays toward it had depth moments
/// `(mean, mean²)` (Chebyshev's inequality; 1: fully visible).
pub fn chebyshev(moments: Vec2, distance: f32) -> f32 {
    let (mean, mean2) = (moments.x, moments.y);
    if !(mean.is_finite() && mean2.is_finite() && distance.is_finite()) || distance <= mean {
        return 1.0;
    }
    let variance = (mean2 - mean * mean).abs();
    let d = distance - mean;
    // Cubed to sharpen the falloff (Majercik et al. 2019).
    (variance / (variance + d * d)).powi(3).clamp(0.0, 1.0)
}

/// A probe's offset from its grid point after this frame's `rays`, starting from `offset`
/// (moved out of geometry and away from close walls, at most 0.45 × `spacing`), and its state.
pub fn relocate(rays: &[ProbeRay], offset: Vec3, spacing: f32) -> (Vec3, ProbeState) {
    if rays.is_empty() || !(spacing.is_finite() && spacing > 0.0) {
        return (Vec3::ZERO, ProbeState::Inactive);
    }
    let hits = rays.iter().filter_map(|r| r.hit.map(|t| (r, t)));
    let backs = hits.clone().filter(|(r, _)| r.backface);
    let back_count = backs.clone().count();
    let hit_count = hits.clone().count();
    let mut moved = offset;
    if back_count * 4 > rays.len() {
        // Buried: step through the nearest backface (Majercik et al. 2021).
        if let Some((r, t)) = nearest(backs) {
            moved += r.dir * (t + RELOCATE_MARGIN * spacing);
        }
    } else if let Some((r, t)) = nearest(hits.filter(|(r, _)| !r.backface)) {
        let keep = MIN_CLEARANCE * spacing;
        if t < keep {
            moved -= r.dir * (keep - t);
        }
    }
    let state = if hit_count == 0 || back_count == rays.len() {
        ProbeState::Inactive
    } else {
        ProbeState::Active
    };
    (moved.clamp_length_max(MAX_OFFSET * spacing), state)
}

/// The nearest of `hits`.
fn nearest<'a>(hits: impl Iterator<Item = (&'a ProbeRay, f32)>) -> Option<(&'a ProbeRay, f32)> {
    hits.min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Probes move at most this many spacings from their grid point.
const MAX_OFFSET: f32 = 0.45;
/// A probe keeps this many spacings from the nearest front face.
const MIN_CLEARANCE: f32 = 0.2;
/// How far past a backface a buried probe moves, in spacings.
const RELOCATE_MARGIN: f32 = 0.05;

/// The weight of the old value when blending in `sample` (adaptive: lower when the value
/// changes a lot, 0.5 at most on a light event).
pub fn hysteresis(old: f32, sample: f32, base: f32, light_event: bool) -> f32 {
    let base = if base.is_finite() {
        base.clamp(0.0, 1.0)
    } else {
        BASE_HYSTERESIS
    };
    let scale = old.abs().max(sample.abs()).max(1e-4);
    let change = (sample - old).abs() / scale;
    let t = if change.is_finite() {
        ((change - CHANGE_LOW) / (CHANGE_HIGH - CHANGE_LOW)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let smooth = t * t * (3.0 - 2.0 * t);
    let h = base + (MIN_HYSTERESIS.min(base) - base) * smooth;
    if light_event { h.min(0.5) } else { h }
}

/// Relative changes below this are noise (full hysteresis).
const CHANGE_LOW: f32 = 0.08;
/// Relative changes above this are real (minimum hysteresis).
const CHANGE_HIGH: f32 = 0.2;
/// The weight of the old value under a large change.
const MIN_HYSTERESIS: f32 = 0.3;
