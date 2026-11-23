//! Spatialization: inverse-distance-clamped attenuation (OpenAL 1.1's model) and equal-power panning.

use concordia_core::math::{Quat, Vec3};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// How a positioned sound fades with distance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(default)]
#[serde(default, deny_unknown_fields)]
pub struct Spatial {
    /// Full volume up to here, in meters.
    #[reflect(range(min = 0.01, max = 10000.0))]
    pub min_distance: f32,
    /// No quieter past here, in meters.
    #[reflect(range(min = 0.01, max = 10000.0))]
    pub max_distance: f32,
}

impl Default for Spatial {
    fn default() -> Self {
        Self {
            min_distance: 1.0,
            max_distance: 50.0,
        }
    }
}

impl Spatial {
    /// Checks 0 < min ≤ max, both finite.
    pub fn validate(&self) -> Result<(), String> {
        let (min, max) = (self.min_distance, self.max_distance);
        if !(min.is_finite() && min > 0.0) {
            return Err(format!("min_distance {min} must be finite and > 0"));
        }
        if !(max.is_finite() && max >= min) {
            return Err(format!(
                "max_distance {max} must be finite and ≥ min_distance {min}"
            ));
        }
        Ok(())
    }
}

/// Where the sound is heard from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    /// Position in meters.
    pub position: Vec3,
    /// The listener's right (+x).
    pub right: Vec3,
    /// The listener's forward (−z).
    pub forward: Vec3,
}

impl Listener {
    /// A listener at `position` turned by `rotation` (right +x, forward −z).
    pub fn new(position: Vec3, rotation: Quat) -> Self {
        Self {
            position,
            right: rotation * Vec3::X,
            forward: rotation * Vec3::NEG_Z,
        }
    }
}

/// The gain at `distance`: min / clamp(d, min, max).
pub fn distance_gain(distance: f32, spatial: &Spatial) -> f32 {
    let min = spatial.min_distance;
    min / distance.clamp(min, spatial.max_distance.max(min))
}

/// Equal-power gains `[left, right]` for a pan in [−1, 1]: θ = (p + 1)π/4, (cos θ, sin θ).
pub fn equal_power(pan: f32) -> [f32; 2] {
    let pan = if pan.is_nan() {
        0.0
    } else {
        pan.clamp(-1.0, 1.0)
    };
    let theta = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
    // f32's cos(π/2) is −2.2e-8; gains are never negative.
    [theta.cos().max(0.0), theta.sin().max(0.0)]
}

/// `[left, right]` gains for a source at `position`; with no listener, centred at full gain.
pub fn spatial_gains(listener: Option<&Listener>, position: Vec3, spatial: &Spatial) -> [f32; 2] {
    let Some(listener) = listener else {
        return equal_power(0.0);
    };
    let offset = position - listener.position;
    let gain = distance_gain(offset.length(), spatial);
    let x = offset.dot(listener.right);
    let z = offset.dot(listener.forward);
    let flat = (x * x + z * z).sqrt();
    let pan = if flat > 0.0 { x / flat } else { 0.0 };
    equal_power(pan).map(|g| g * gain)
}
