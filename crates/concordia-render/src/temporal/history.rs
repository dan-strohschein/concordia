//! Per-view temporal state (M4 4.6 plan Q6): the history textures (output size), last frame's
//! linear depth (internal size), the jitter index, the exposure record and the motion history.

use concordia_rhi::{Device, Texture, TextureUsage};

use super::{FrameExposure, Temporal};
use crate::RenderError;
use crate::pipelines::HDR;
use crate::renderer::desc;

/// What a frame keeps for the next one's disocclusion test: linear depth (half float is 0.05 %
/// relative, well inside the 1 % gap) and motion length.
pub(crate) const KEPT: concordia_rhi::Format = concordia_rhi::Format::Rg16Float;

/// MetalFX's reactive mask format.
pub(crate) const REACTIVE: concordia_rhi::Format = concordia_rhi::Format::R8Unorm;

/// A view's GPU objects to free once the GPU is done with them.
#[derive(Debug, Default)]
pub(crate) struct Freed {
    pub(crate) textures: Vec<Texture>,
    pub(crate) scaler: Option<concordia_rhi::TemporalScaler>,
}

/// Which view a history belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum HistoryKey {
    /// Offscreen renders (`Renderer::select_history`).
    Offscreen(u64),
    /// A window surface's presents.
    Surface(concordia_rhi::Surface),
}

impl HistoryKey {
    /// A number naming the view (the same key gives the same number).
    pub(crate) fn id(self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        self.hash(&mut h);
        h.finish()
    }
}

/// The ping-ponged textures of one view.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HistoryTextures {
    /// History (`Rgba16Float`, output size; alpha: accumulated samples).
    pub(crate) history: [Texture; 2],
    /// Linear depth and motion length in pixels ([`KEPT`], internal size).
    pub(crate) depth: [Texture; 2],
    /// MetalFX's reactive mask (`R8Unorm`, internal size; the `metalfx` backend only).
    pub(crate) reactive: Option<Texture>,
    /// Which of each pair holds last frame's.
    pub(crate) read: usize,
    /// Whether last frame's were written (their layout is `ShaderRead`).
    pub(crate) written: bool,
}

/// The settings, output size and internal size the textures were made for.
type Shape = (Temporal, (u32, u32), (u32, u32));

/// One view's temporal state.
pub(crate) struct ViewHistory {
    pub(crate) key: HistoryKey,
    /// The settings and sizes the textures were made for (output, internal).
    shape: Option<Shape>,
    pub(crate) textures: Option<HistoryTextures>,
    /// MetalFX's scaler (it keeps its own history; the `metalfx` backend only).
    pub(crate) scaler: Option<concordia_rhi::TemporalScaler>,
    /// Frames accumulated since the last reset (the jitter index).
    pub(crate) frames: u64,
    pub(crate) exposure: Option<FrameExposure>,
    /// The renderer frame this view was last drawn in.
    pub(crate) last_used: u64,
    /// Last frame's transforms, for this view's motion vectors.
    pub(crate) motion: crate::motion::MotionHistory,
}

impl ViewHistory {
    /// A view with no history.
    pub(crate) fn new(key: HistoryKey) -> Self {
        Self {
            key,
            shape: None,
            textures: None,
            scaler: None,
            frames: 0,
            exposure: None,
            last_used: 0,
            motion: Default::default(),
        }
    }

    /// Forgets the accumulated history and motion: the next frame starts fresh.
    pub(crate) fn forget(&mut self) {
        self.pause();
        self.motion.reset();
    }

    /// Forgets the accumulated history but keeps the motion (a frame without temporal).
    pub(crate) fn pause(&mut self) {
        self.frames = 0;
        self.exposure = None;
        if let Some(t) = &mut self.textures {
            t.written = false;
        }
    }

    /// The textures and scaler, for freeing once the GPU is done with them.
    pub(crate) fn take_textures(&mut self) -> Freed {
        self.shape = None;
        let textures = self.textures.take().map_or_else(Vec::new, |t| {
            t.history
                .into_iter()
                .chain(t.depth)
                .chain(t.reactive)
                .collect()
        });
        Freed {
            textures,
            scaler: self.scaler.take(),
        }
    }

    /// Makes the textures (and MetalFX's scaler, for `metalfx`) fit `temporal` at `output` from
    /// `internal`, resetting on any change; returns what to free.
    pub(crate) fn fit(
        &mut self,
        device: &dyn Device,
        temporal: Temporal,
        output: (u32, u32),
        internal: (u32, u32),
    ) -> Result<Freed, RenderError> {
        let shape = Some((temporal, output, internal));
        if self.shape == shape && self.textures.is_some() {
            return Ok(Freed::default());
        }
        self.pause();
        let freed = self.take_textures();
        let metalfx = temporal.backend == super::TemporalBackend::metalfx;
        let usage = TextureUsage::SAMPLED | TextureUsage::STORAGE | TextureUsage::BINDLESS;
        let size = |(w, h): (u32, u32)| {
            (
                std::num::NonZeroU32::new(w).unwrap_or(std::num::NonZeroU32::MIN),
                std::num::NonZeroU32::new(h).unwrap_or(std::num::NonZeroU32::MIN),
            )
        };
        let one = concordia_rhi::SampleCount::X1;
        let make_with = |s, format, usage, label: &str| {
            device.create_texture(&desc(size(s), format, one, usage, label))
        };
        let make = |s, format, label: &str| make_with(s, format, usage, label);
        // MetalFX writes its output as a render target too.
        let out = match metalfx {
            true => usage | TextureUsage::RENDER_TARGET,
            false => usage,
        };
        let history = [
            make_with(output, HDR, out, "temporal history a")?,
            make_with(output, HDR, out, "temporal history b")?,
        ];
        let depth = [
            make(internal, KEPT, "temporal depth a")?,
            make(internal, KEPT, "temporal depth b")?,
        ];
        let reactive = match metalfx {
            true => Some(make(internal, REACTIVE, "temporal reactive")?),
            false => None,
        };
        if metalfx {
            self.scaler = Some(device.create_temporal_scaler(&super::metalfx::scaler_desc(
                size(internal),
                size(output),
            ))?);
        }
        self.textures = Some(HistoryTextures {
            history,
            depth,
            reactive,
            read: 0,
            written: false,
        });
        self.shape = shape;
        Ok(freed)
    }

    /// Whether last frame left history to reproject.
    pub(crate) fn has_history(&self) -> bool {
        self.frames > 0 && self.textures.as_ref().is_some_and(|t| t.written)
    }

    /// After a frame that wrote the textures: swap them and count the frame.
    pub(crate) fn advance(&mut self) {
        if let Some(t) = &mut self.textures {
            t.read ^= 1;
            t.written = true;
        }
        self.frames += 1;
    }
}
