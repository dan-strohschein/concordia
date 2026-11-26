//! `[render] temporal` and `upscale`, and what follows from them: the internal size and mip bias.

use std::num::NonZeroU32;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Subtracted from `log2(internal / output)` so textures stay slightly sharper than the internal
/// size alone would pick (provisional; 4.6 plan Q4).
pub const MIP_BIAS_EPSILON: f32 = 0.5;

/// Which temporal resolve runs (`[render] temporal`).
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum TemporalBackend {
    /// None: the frame renders at the output size with the camera's MSAA.
    #[default]
    off,
    /// The engine's own TAA/TAAU (every backend).
    own,
    /// Apple's MetalFX temporal scaler (falls back to `own` where it is unavailable).
    metalfx,
}

/// Output size over internal size (`[render] upscale`: 1.0, 1.5 or 2.0).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub enum Upscale {
    /// Native: TAA only.
    #[default]
    X1,
    /// 1.5× (the M4 gate's 1706 × 960 → 2560 × 1440).
    X1_5,
    /// 2×.
    X2,
}

/// An `upscale` that isn't 1.0, 1.5 or 2.0.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("upscale must be 1.0, 1.5 or 2.0, not {0}")]
pub struct UpscaleError(pub f64);

impl Upscale {
    /// Output size over internal size.
    pub fn ratio(self) -> f32 {
        match self {
            Self::X1 => 1.0,
            Self::X1_5 => 1.5,
            Self::X2 => 2.0,
        }
    }

    /// Jitter phases: `ceil(8 · ratio²)` (8, 18, 32).
    pub fn phases(self) -> u32 {
        match self {
            Self::X1 => 8,
            Self::X1_5 => 18,
            Self::X2 => 32,
        }
    }

    /// `n · 1 / ratio`, rounded down (exact: the ratios are 1, 3/2 and 2).
    fn shrink(self, n: u32) -> u32 {
        match self {
            Self::X1 => n,
            Self::X1_5 => (u64::from(n) * 2 / 3) as u32,
            Self::X2 => n / 2,
        }
    }
}

impl TryFrom<f64> for Upscale {
    type Error = UpscaleError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        [Self::X1, Self::X1_5, Self::X2]
            .into_iter()
            .find(|u| f64::from(u.ratio()) == value)
            .ok_or(UpscaleError(value))
    }
}

impl From<Upscale> for f64 {
    fn from(upscale: Upscale) -> Self {
        upscale.ratio() as f64
    }
}

/// The temporal settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Temporal {
    /// Which resolve runs.
    pub backend: TemporalBackend,
    /// Output over internal size (ignored while `backend` is `off`).
    pub upscale: Upscale,
}

impl Temporal {
    /// Whether a temporal resolve runs.
    pub fn is_on(&self) -> bool {
        self.backend != TemporalBackend::off
    }
}

/// The size the scene renders at for `output`: `floor(output / ratio)`, at least 1.
pub fn internal_size(
    output: (NonZeroU32, NonZeroU32),
    upscale: Upscale,
) -> (NonZeroU32, NonZeroU32) {
    let shrink =
        |n: NonZeroU32| NonZeroU32::new(upscale.shrink(n.get())).unwrap_or(NonZeroU32::MIN);
    (shrink(output.0), shrink(output.1))
}

/// The texture mip bias at `internal` of `output` pixels across: `log2(internal / output) − ε`.
pub fn mip_bias(internal: NonZeroU32, output: NonZeroU32) -> f32 {
    (internal.get() as f32 / output.get() as f32).log2() - MIP_BIAS_EPSILON
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(w: u32, h: u32) -> (NonZeroU32, NonZeroU32) {
        (NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap())
    }

    #[test]
    fn internal_size_follows_the_ratio() {
        assert_eq!(
            internal_size(size(2560, 1440), Upscale::X1_5),
            size(1706, 960)
        );
        assert_eq!(
            internal_size(size(2560, 1440), Upscale::X2),
            size(1280, 720)
        );
        assert_eq!(
            internal_size(size(2560, 1440), Upscale::X1),
            size(2560, 1440)
        );
        assert_eq!(internal_size(size(1, 1), Upscale::X2), size(1, 1));
        assert_eq!(internal_size(size(3, 2), Upscale::X1_5), size(2, 1));
    }

    #[test]
    fn mip_bias_follows_the_ratio() {
        let bias = |i, o| mip_bias(NonZeroU32::new(i).unwrap(), NonZeroU32::new(o).unwrap());
        assert_eq!(bias(1280, 2560), -1.0 - MIP_BIAS_EPSILON);
        assert_eq!(bias(1920, 1920), -MIP_BIAS_EPSILON);
        let expected = (1706f32 / 2560.0).log2() - MIP_BIAS_EPSILON;
        assert!((bias(1706, 2560) - expected).abs() < 1e-6);
    }

    #[test]
    fn upscale_ratios_and_phases() {
        let all = [
            (Upscale::X1, 1.0, 8),
            (Upscale::X1_5, 1.5, 18),
            (Upscale::X2, 2.0, 32),
        ];
        for (u, ratio, phases) in all {
            assert_eq!((u.ratio(), u.phases()), (ratio, phases), "{u:?}");
            assert_eq!(Upscale::try_from(ratio as f64), Ok(u));
        }
        assert_eq!(Upscale::try_from(1.25), Err(UpscaleError(1.25)));
        assert!(Upscale::try_from(f64::NAN).is_err());
    }

    #[test]
    fn temporal_settings_read_from_text() {
        let t: Temporal = serde_json::from_str(r#"{"backend": "own", "upscale": 1.5}"#).unwrap();
        assert_eq!(
            t,
            Temporal {
                backend: TemporalBackend::own,
                upscale: Upscale::X1_5
            }
        );
        assert!(t.is_on());
        let off: Temporal = serde_json::from_str("{}").unwrap();
        assert_eq!(off, Temporal::default());
        assert!(!off.is_on());
        let bad = serde_json::from_str::<Temporal>(r#"{"upscale": 3.0}"#)
            .unwrap_err()
            .to_string();
        assert!(
            bad.contains("upscale must be 1.0, 1.5 or 2.0, not 3"),
            "{bad}"
        );
        let round = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Temporal>(&round).unwrap(), t);
    }
}
