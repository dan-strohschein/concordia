//! `[render] shadows` and `[render.vsm]`.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Which shadow technique the sun uses (`[render] shadows`).
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum ShadowTechnique {
    /// Cascaded shadow maps (M1).
    #[default]
    csm,
    /// Virtual shadow maps (falls back to `csm` where the device can't run them).
    vsm,
}

/// The sun's shadow settings a renderer runs with: the technique and `[render.vsm]`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Shadows {
    /// `csm` or `vsm`.
    pub technique: ShadowTechnique,
    /// The virtual shadow maps' pool, levels and biases.
    pub vsm: VsmSettings,
}

/// `[render.vsm]`: the virtual shadow maps' sizes and biases (validated when read).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawVsm", into = "RawVsm")]
pub struct VsmSettings {
    /// Physical pages (128² texels, 64 KB each), 64..=16 384.
    pub pool_pages: u32,
    /// Clipmap levels, 1..=24.
    pub levels: u8,
    /// Level offset, −4..=4: −1 finer, +1 coarser. −1 by default: two texels per pixel across, so
    /// the light's PCF kernel (three texels or more) stays within about a pixel (4.7a's reference
    /// test: FLIP 0.0149 against 0.0241 at 0 on `sun_courtyard_iso`).
    pub resolution_bias: f32,
    /// Distance the coarse floor covers in metres, (0, 100 000].
    pub max_distance: f32,
    /// Receiver depth bias in shadow texels, 0..=10.
    pub depth_bias: f32,
    /// Receiver offset along the normal in shadow texels, 0..=10.
    pub normal_bias: f32,
}

impl VsmSettings {
    /// Checks every field's range (as reading `[render.vsm]` does).
    pub fn validate(&self) -> Result<(), String> {
        Self::try_from(RawVsm::from(*self)).map(|_| ())
    }
}

impl Default for VsmSettings {
    fn default() -> Self {
        Self {
            pool_pages: 4096,
            levels: 16,
            resolution_bias: -1.0,
            max_distance: 1000.0,
            depth_bias: 1.0,
            normal_bias: 1.0,
        }
    }
}

/// `[render.vsm]` as written, before validation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawVsm {
    pool_pages: u32,
    levels: u8,
    resolution_bias: f32,
    max_distance: f32,
    depth_bias: f32,
    normal_bias: f32,
}

impl Default for RawVsm {
    fn default() -> Self {
        VsmSettings::default().into()
    }
}

impl From<VsmSettings> for RawVsm {
    fn from(s: VsmSettings) -> Self {
        Self {
            pool_pages: s.pool_pages,
            levels: s.levels,
            resolution_bias: s.resolution_bias,
            max_distance: s.max_distance,
            depth_bias: s.depth_bias,
            normal_bias: s.normal_bias,
        }
    }
}

impl TryFrom<RawVsm> for VsmSettings {
    type Error = String;

    fn try_from(r: RawVsm) -> Result<Self, String> {
        within("pool_pages", f64::from(r.pool_pages), 64.0, 16_384.0)?;
        within("levels", f64::from(r.levels), 1.0, 24.0)?;
        within("resolution_bias", f64::from(r.resolution_bias), -4.0, 4.0)?;
        within("depth_bias", f64::from(r.depth_bias), 0.0, 10.0)?;
        within("normal_bias", f64::from(r.normal_bias), 0.0, 10.0)?;
        if !(r.max_distance > 0.0 && r.max_distance <= 100_000.0) {
            return Err(format!(
                "max_distance must be in (0, 100000], not {}",
                r.max_distance
            ));
        }
        Ok(Self {
            pool_pages: r.pool_pages,
            levels: r.levels,
            resolution_bias: r.resolution_bias,
            max_distance: r.max_distance,
            depth_bias: r.depth_bias,
            normal_bias: r.normal_bias,
        })
    }
}

/// `value` lies in `lo..=hi` (NaN does not).
fn within(key: &str, value: f64, lo: f64, hi: f64) -> Result<(), String> {
    match (lo..=hi).contains(&value) {
        true => Ok(()),
        false => Err(format!("{key} must be {lo}..={hi}, not {value}")),
    }
}
