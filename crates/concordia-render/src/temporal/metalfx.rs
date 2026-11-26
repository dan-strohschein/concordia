//! The `metalfx` backend (M4 4.6b, ADR-0103): MetalFX's temporal scaler through the RHI's
//! `TemporalScale`. `temporal.reactive_mask` copies the HDR alpha into the scaler's `R8Unorm`
//! mask, then `temporal.metalfx` scales into the view's history texture, which the tonemap reads.
//! The scaler keeps its own history; the view keeps the scaler.

use std::num::NonZeroU32;
use std::sync::Arc;

use concordia_rhi::{
    Command, Device, Format, Layout, SampleCount, TemporalScale, TemporalScaler,
    TemporalScalerDesc, TextureUsage,
};

use super::history::{HistoryTextures, REACTIVE};
use super::own::{OwnPipelines, dispatch};
use super::{TemporalInputs, TemporalUpscaler};
use crate::RenderError;
use crate::graph::{GraphTexture, PassContext, RenderGraph, Use};
use crate::pipelines::HDR;
use crate::renderer::desc;

/// The scaler a view needs for `internal` → `output` (the forward pass's formats).
pub(crate) fn scaler_desc(
    internal: (NonZeroU32, NonZeroU32),
    output: (NonZeroU32, NonZeroU32),
) -> TemporalScalerDesc {
    TemporalScalerDesc {
        input: [internal.0, internal.1],
        output: [output.0, output.1],
        color: HDR,
        depth: Format::Depth32Float,
        motion: Format::Rg16Float,
        reactive: Some(REACTIVE),
        output_format: HDR,
    }
}

/// MetalFX's temporal scaler for one view.
pub(crate) struct MetalFxTemporal {
    pub(crate) pipelines: OwnPipelines,
    pub(crate) device: Arc<dyn Device>,
    pub(crate) scaler: TemporalScaler,
}

impl TemporalUpscaler for MetalFxTemporal {
    fn add_pass(
        &self,
        graph: &mut RenderGraph,
        inputs: &TemporalInputs,
        textures: &HistoryTextures,
        has_history: bool,
    ) -> Result<GraphTexture, RenderError> {
        let Some(reactive) = textures.reactive else {
            return Err(RenderError::Gpu(concordia_rhi::RhiError::invalid(
                "temporal",
                "the metalfx backend's view has no reactive mask",
            )));
        };
        let usage = TextureUsage::SAMPLED | TextureUsage::STORAGE | TextureUsage::BINDLESS;
        let one = SampleCount::X1;
        let mask = graph.import_texture(
            "temporal reactive",
            reactive,
            desc(inputs.internal, REACTIVE, one, usage, "temporal reactive"),
            Layout::Undefined,
            Some(Layout::ShaderRead),
        );
        let out = graph.import_texture(
            "temporal out",
            textures.history[textures.read ^ 1],
            desc(
                inputs.output,
                HDR,
                one,
                usage | TextureUsage::RENDER_TARGET,
                "temporal out",
            ),
            Layout::Undefined,
            Some(Layout::ShaderRead),
        );
        let (device, pipelines, i) = (self.device.clone(), self.pipelines, *inputs);
        graph.add_pass_recorded(
            "temporal.reactive_mask",
            |p| {
                p.uses(i.color, Use::SampledRead);
                p.uses(mask, Use::StorageWrite);
            },
            Some(Box::new(move |ctx: &mut PassContext<'_>| {
                let index = |t| device.texture_index(ctx.texture(t)).map(|i| i.0);
                let words = [
                    index(i.color)?,
                    index(mask)?,
                    i.internal.0.get(),
                    i.internal.1.get(),
                ];
                let push = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                ctx.commands
                    .push(Command::SetPipeline(pipelines.reactive_mask));
                ctx.commands.push(Command::PushConstants(push));
                ctx.commands.push(dispatch(i.internal));
                Ok(())
            })),
        );
        let scaler = self.scaler;
        graph.add_pass_recorded(
            "temporal.metalfx",
            |p| {
                for t in [i.color, i.depth, i.motion, mask] {
                    p.uses(t, Use::SampledRead);
                }
                p.uses(out, Use::StorageWrite);
            },
            Some(Box::new(move |ctx: &mut PassContext<'_>| {
                let (w, h) = (i.internal.0.get() as f32, i.internal.1.get() as f32);
                ctx.commands.push(Command::TemporalScale(TemporalScale {
                    scaler,
                    color: ctx.texture(i.color),
                    depth: ctx.texture(i.depth),
                    motion: ctx.texture(i.motion),
                    reactive: Some(ctx.texture(mask)),
                    output: ctx.texture(out),
                    jitter: i.jitter,
                    // Our motion is now minus then in UV; MetalFX wants input pixels toward then.
                    motion_scale: [-w, -h],
                    pre_exposure: i.exposure.relative(),
                    reset: !has_history,
                }));
                Ok(())
            })),
        );
        Ok(out)
    }
}
