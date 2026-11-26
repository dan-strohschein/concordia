//! Temporal anti-aliasing and upscaling (M4 4.6, ADR-0103): the settings, the internal size, the
//! jitter sequence, the mip bias and the exposure record.

mod api;
mod exposure;
pub(crate) mod history;
mod jitter;
pub(crate) mod own;
mod settings;

use std::num::NonZeroU32;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::RenderError;
use crate::graph::{GraphTexture, RenderGraph};

pub use api::{RenderSetTemporal, RenderSetTemporalParams, RenderSetTemporalResult};
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

/// Parts of the own resolve that can be switched off (tests show what each prevents).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemporalSwitches {
    /// Reset history where last frame's depth says the surface was hidden.
    pub disocclusion: bool,
    /// Trust this frame where translucent draws cover (the HDR alpha).
    pub reactive: bool,
}

impl Default for TemporalSwitches {
    fn default() -> Self {
        Self {
            disocclusion: true,
            reactive: true,
        }
    }
}

/// What the temporal resolve did in a frame (`render.stats` `temporal`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct TemporalStats {
    /// What ran: `off` (temporal off, or a debug view), `own` or `metalfx`.
    pub backend: TemporalBackend,
    /// The scene's size (the output's with temporal off).
    pub internal: [u32; 2],
    /// The image's size.
    pub output: [u32; 2],
    /// The jitter phase (frames since the history started).
    pub phase: u64,
    /// No history was reprojected (the first frame, or after a change or reset).
    pub reset: bool,
}
