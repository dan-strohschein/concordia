//! The own TAA/TAAU (`temporal.slang`): `temporal.resolve` per output pixel into the history,
//! then `temporal.keep_depth` keeps this frame's linear depth for the next frame.

use std::sync::Arc;

use concordia_rhi::{Command, Device, Layout, Pipeline, SampleCount, TextureUsage};

use super::history::HistoryTextures;
use super::{TemporalInputs, TemporalUpscaler};
use crate::RenderError;
use crate::graph::{GraphTexture, PassContext, RenderGraph, Use};
use crate::pipelines::HDR;
use crate::renderer::desc;

/// Flags of `TaaPush` (mirrors `temporal.slang`).
const HAS_HISTORY: u32 = 1;
const DISOCCLUSION: u32 = 2;
const REACTIVE: u32 = 4;

/// The two compute pipelines.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OwnPipelines {
    pub(crate) resolve: Pipeline,
    pub(crate) keep_depth: Pipeline,
}

/// The engine's own temporal resolve.
pub(crate) struct OwnTaau {
    pub(crate) pipelines: OwnPipelines,
    pub(crate) device: Arc<dyn Device>,
    /// The disocclusion test and the reactive mask (both on; tests switch them off).
    pub(crate) disocclusion: bool,
    pub(crate) reactive: bool,
}

impl TemporalUpscaler for OwnTaau {
    fn add_pass(
        &self,
        graph: &mut RenderGraph,
        inputs: &TemporalInputs,
        textures: &HistoryTextures,
        has_history: bool,
    ) -> Result<GraphTexture, RenderError> {
        let (read, write) = (textures.read, textures.read ^ 1);
        let before = match textures.written {
            true => Layout::ShaderRead,
            false => Layout::Undefined,
        };
        let usage = TextureUsage::SAMPLED | TextureUsage::STORAGE | TextureUsage::BINDLESS;
        let one = SampleCount::X1;
        let import = |graph: &mut RenderGraph, name: &str, t, size, format, initial| {
            graph.import_texture(
                name,
                t,
                desc(size, format, one, usage, name),
                initial,
                Some(Layout::ShaderRead),
            )
        };
        let r32 = concordia_rhi::Format::R32Float;
        let history = import(
            graph,
            "temporal history",
            textures.history[read],
            inputs.output,
            HDR,
            before,
        );
        let out = import(
            graph,
            "temporal out",
            textures.history[write],
            inputs.output,
            HDR,
            Layout::Undefined,
        );
        let prev_depth = import(
            graph,
            "temporal depth",
            textures.depth[read],
            inputs.internal,
            r32,
            before,
        );
        let next_depth = import(
            graph,
            "temporal next depth",
            textures.depth[write],
            inputs.internal,
            r32,
            Layout::Undefined,
        );
        let mut flags = if has_history { HAS_HISTORY } else { 0 };
        flags |= if self.disocclusion { DISOCCLUSION } else { 0 };
        flags |= if self.reactive { REACTIVE } else { 0 };
        let (device, pipelines, i) = (self.device.clone(), self.pipelines, *inputs);
        graph.add_pass_recorded(
            "temporal.resolve",
            |p| {
                for t in [i.color, i.depth, i.motion, history, prev_depth] {
                    p.uses(t, Use::SampledRead);
                }
                p.uses(out, Use::StorageWrite);
            },
            Some(Box::new(move |ctx: &mut PassContext<'_>| {
                let index = |t| device.texture_index(ctx.texture(t)).map(|i| i.0);
                let words = [
                    index(i.color)?,
                    index(i.depth)?,
                    index(i.motion)?,
                    index(history)?,
                    index(prev_depth)?,
                    index(out)?,
                    flags,
                    0,
                ];
                let floats = [
                    i.jitter[0],
                    i.jitter[1],
                    i.internal.0.get() as f32,
                    i.internal.1.get() as f32,
                    i.output.0.get() as f32,
                    i.output.1.get() as f32,
                    i.exposure.history_scale(),
                    i.max_count,
                ];
                let mut push: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                push.extend(
                    floats
                        .iter()
                        .chain(&i.depth_terms)
                        .flat_map(|f| f.to_le_bytes()),
                );
                ctx.commands.push(Command::SetPipeline(pipelines.resolve));
                ctx.commands.push(Command::PushConstants(push));
                ctx.commands.push(dispatch(i.output));
                Ok(())
            })),
        );
        let device = self.device.clone();
        graph.add_pass_recorded(
            "temporal.keep_depth",
            |p| {
                p.uses(i.depth, Use::SampledRead);
                p.uses(next_depth, Use::StorageWrite);
                p.side_effect();
            },
            Some(Box::new(move |ctx: &mut PassContext<'_>| {
                let index = |t| device.texture_index(ctx.texture(t)).map(|i| i.0);
                let words = [
                    index(i.depth)?,
                    index(next_depth)?,
                    i.internal.0.get(),
                    i.internal.1.get(),
                ];
                let mut push: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                push.extend(i.depth_terms.iter().flat_map(|f| f.to_le_bytes()));
                ctx.commands
                    .push(Command::SetPipeline(pipelines.keep_depth));
                ctx.commands.push(Command::PushConstants(push));
                ctx.commands.push(dispatch(i.internal));
                Ok(())
            })),
        );
        Ok(out)
    }
}

/// One 8 × 8 group per tile of `size`.
fn dispatch(size: (std::num::NonZeroU32, std::num::NonZeroU32)) -> Command {
    Command::Dispatch {
        groups: [size.0.get().div_ceil(8), size.1.get().div_ceil(8), 1],
    }
}
