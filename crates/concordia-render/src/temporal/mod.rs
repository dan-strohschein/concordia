//! Temporal anti-aliasing and upscaling (M4 4.6, ADR-0103): the settings, the internal size, the
//! jitter sequence, the mip bias and the exposure record.

mod exposure;
pub(crate) mod history;
mod jitter;
pub(crate) mod own;
mod settings;

use std::num::NonZeroU32;

use crate::RenderError;
use crate::graph::{GraphTexture, RenderGraph};

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

/// What a temporal resolve reads this frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TemporalInputs {
    /// Internal HDR (alpha: the reactive mask), depth (reverse-Z) and motion.
    pub(crate) color: GraphTexture,
    pub(crate) depth: GraphTexture,
    pub(crate) motion: GraphTexture,
    pub(crate) internal: (NonZeroU32, NonZeroU32),
    pub(crate) output: (NonZeroU32, NonZeroU32),
    /// This frame's jitter in input pixels (x right, y down).
    pub(crate) jitter: [f32; 2],
    pub(crate) exposure: FrameExposure,
    /// The most samples a pixel accumulates (the jitter phases).
    pub(crate) max_count: f32,
    /// Linearizes depth: `z = −(a·d + b) / (c·d + e)` from the inverse projection.
    pub(crate) depth_terms: [f32; 4],
}

/// A temporal resolve: reads the frame's internal targets and a view's history, writes the next
/// history at the output size and returns it (what the tonemap reads).
pub(crate) trait TemporalUpscaler {
    /// Adds its passes to `graph`.
    fn add_pass(
        &self,
        graph: &mut RenderGraph,
        inputs: &TemporalInputs,
        textures: &history::HistoryTextures,
        has_history: bool,
    ) -> Result<GraphTexture, RenderError>;
}

/// The depth-linearizing terms of projection `proj` (its inverse's z and w rows' depth and constant
/// columns).
pub(crate) fn depth_terms(proj: &concordia_core::math::Mat4) -> [f32; 4] {
    let inv = proj.inverse();
    [inv.z_axis.z, inv.w_axis.z, inv.z_axis.w, inv.w_axis.w]
}
