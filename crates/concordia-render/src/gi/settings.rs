//! GI's settings (`[render] gi`, `[render.gi]`; M4 4.8).

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::RenderError;

/// How diffuse light bounces (`[render] gi`).
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum GiTechnique {
    /// Direct light only (plus the environment's SH9 ambient).
    #[default]
    off,
    /// DDGI-style probes around the camera.
    probes,
}

/// The probe volume (`[render.probes]`; validated when read).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawGiSettings", into = "RawGiSettings")]
pub struct GiSettings {
    /// Rays per probe per frame (1..=512).
    pub rays: u32,
    /// Probes along x, y and z (each 1..=256).
    pub probes: [u32; 3],
    /// Metres between probes.
    pub spacing: f32,
    /// A fixed centre for the volume (`None`: it follows the camera).
    pub center: Option<[f32; 3]>,
}

impl Default for GiSettings {
    fn default() -> Self {
        Self {
            rays: 256,
            probes: [32, 8, 32],
            spacing: 1.5,
            center: None,
        }
    }
}

impl GiSettings {
    /// Refuses values out of range.
    pub fn validate(&self) -> Result<(), RenderError> {
        let bad = |m: String| Err(RenderError::InvalidSettings(format!("[render.probes] {m}")));
        if !(1..=512).contains(&self.rays) {
            return bad(format!("rays {} must be 1..=512", self.rays));
        }
        if self.probes.iter().any(|&n| !(1..=256).contains(&n)) {
            return bad(format!("probes {:?} must each be 1..=256", self.probes));
        }
        if !(self.spacing.is_finite() && self.spacing > 0.0 && self.spacing <= 100.0) {
            return bad(format!("spacing {} must be in (0, 100]", self.spacing));
        }
        if self
            .center
            .is_some_and(|c| c.iter().any(|v| !v.is_finite()))
        {
            return bad(format!("center {:?} must be finite", self.center));
        }
        Ok(())
    }
}

/// `[render.probes]` as written, before validation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawGiSettings {
    rays: u32,
    probes: [u32; 3],
    spacing: f32,
    center: Option<[f32; 3]>,
}

impl Default for RawGiSettings {
    fn default() -> Self {
        GiSettings::default().into()
    }
}

impl From<GiSettings> for RawGiSettings {
    fn from(s: GiSettings) -> Self {
        Self {
            rays: s.rays,
            probes: s.probes,
            spacing: s.spacing,
            center: s.center,
        }
    }
}

impl TryFrom<RawGiSettings> for GiSettings {
    type Error = String;

    fn try_from(r: RawGiSettings) -> Result<Self, String> {
        let s = Self {
            rays: r.rays,
            probes: r.probes,
            spacing: r.spacing,
            center: r.center,
        };
        s.validate().map_err(|e| e.to_string())?;
        Ok(s)
    }
}

/// GI's technique and settings.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Gi {
    /// The technique.
    pub technique: GiTechnique,
    /// The probe volume.
    pub settings: GiSettings,
}
