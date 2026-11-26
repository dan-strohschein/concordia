//! Temporal anti-aliasing and upscaling (M4 4.6, ADR-0103): the settings, the internal size, the
//! jitter sequence, the mip bias and the exposure record.

mod exposure;
mod jitter;
mod settings;

pub use exposure::FrameExposure;
pub use jitter::{halton, jitter, ndc_offset};
pub use settings::{
    MIP_BIAS_EPSILON, Temporal, TemporalBackend, Upscale, UpscaleError, internal_size, mip_bias,
};

/// A fixed sub-pixel offset and texture mip bias for the camera, used instead of the temporal
/// sequence (supersampled references and tests).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SubpixelOverride {
    /// Offset in pixels of the rendered size (x right, y down).
    pub offset: [f32; 2],
    /// Added to every material texture's mip level.
    pub mip_bias: f32,
}
