//! A temporal upscaler the device provides (MetalFX's on Apple; M4 4.6b, ADR-0103): its
//! descriptor and the `TemporalScale` command's arguments.

use std::num::NonZeroU32;

use crate::error::RhiError;
use crate::format::Format;
use crate::handle::{TemporalScaler, Texture};

/// A temporal scaler to create: the input (render) and output sizes and the textures' formats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalScalerDesc {
    /// Input width and height.
    pub input: [NonZeroU32; 2],
    /// Output width and height (at least the input's).
    pub output: [NonZeroU32; 2],
    /// The input color's format.
    pub color: Format,
    /// The input depth's format.
    pub depth: Format,
    /// The motion vectors' format.
    pub motion: Format,
    /// The reactive mask's format, if the scaler reads one.
    pub reactive: Option<Format>,
    /// The output's format.
    pub output_format: Format,
}

impl TemporalScalerDesc {
    /// Checks sizes and formats: the output at least the input on each axis, float or unorm
    /// RGBA color and output, `Depth32Float` depth, `Rg16Float` motion, an `R8Unorm` mask.
    pub fn validate(&self) -> Result<(), RhiError> {
        let bad = |reason: String| Err(RhiError::invalid("temporal scaler desc", reason));
        if self.output[0] < self.input[0] || self.output[1] < self.input[1] {
            return bad(format!(
                "output {}×{} is smaller than the input {}×{}",
                self.output[0], self.output[1], self.input[0], self.input[1]
            ));
        }
        let rgba = [Format::Rgba16Float, Format::Rgba32Float, Format::Rgba8Unorm];
        let formats = [
            ("color", Some(self.color), &rgba[..]),
            ("depth", Some(self.depth), &[Format::Depth32Float][..]),
            ("motion", Some(self.motion), &[Format::Rg16Float][..]),
            ("reactive", self.reactive, &[Format::R8Unorm][..]),
            ("output", Some(self.output_format), &rgba[..]),
        ];
        for (name, format, allowed) in formats {
            if let Some(f) = format
                && !allowed.contains(&f)
            {
                return bad(format!("{name} format {f:?} is not one of {allowed:?}"));
            }
        }
        Ok(())
    }
}

/// One temporal scale: this frame's inputs into `output`.
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalScale {
    /// The scaler.
    pub scaler: TemporalScaler,
    /// This frame's color at the input size.
    pub color: Texture,
    /// This frame's depth (reverse-Z).
    pub depth: Texture,
    /// Motion vectors (input size).
    pub motion: Texture,
    /// The reactive mask, if the scaler reads one.
    pub reactive: Option<Texture>,
    /// The upscaled result.
    pub output: Texture,
    /// This frame's sub-pixel jitter, in input pixels (each within ±1).
    pub jitter: [f32; 2],
    /// Multiplies the motion texture's values into input pixels toward last frame's position.
    pub motion_scale: [f32; 2],
    /// The exposure already applied to `color` (positive).
    pub pre_exposure: f32,
    /// Starts the history afresh.
    pub reset: bool,
}

impl TemporalScale {
    /// Checks the scalar arguments.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self.jitter.iter().all(|j| j.is_finite() && j.abs() <= 1.0) {
            return Err(format!("jitter {:?} is not within ±1 pixel", self.jitter));
        }
        if !self.motion_scale.iter().all(|m| m.is_finite()) {
            return Err(format!(
                "motion scale {:?} is not finite",
                self.motion_scale
            ));
        }
        if !(self.pre_exposure.is_finite() && self.pre_exposure > 0.0) {
            return Err(format!(
                "pre-exposure {} is not positive",
                self.pre_exposure
            ));
        }
        Ok(())
    }
}
