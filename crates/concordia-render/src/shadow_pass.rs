//! GPU side of cascaded shadows: persistent shadow maps, caster draws and the graph passes.

use std::num::NonZeroU32;

use concordia_core::math::Vec3;
use concordia_rhi::{
    ClearValue, Command, DepthAttachment, Device, Layout, LoadOp, Pipeline, RenderPassDesc,
    SampleCount, StoreOp, Texture, TextureDesc, TextureUsage,
};

use crate::RenderError;
use crate::graph::{GraphTexture, PassContext, RenderGraph, Use};
use crate::pipelines::DEPTH;

/// One depth texture per cascade, kept across frames (re-created when the size or count changes).
#[derive(Default)]
pub(crate) struct ShadowMaps {
    resolution: u32,
    maps: Vec<(Texture, u32)>,
}

impl ShadowMaps {
    /// Textures and bindless indices for `count` cascades at `resolution`.
    pub(crate) fn ensure(
        &mut self,
        device: &dyn Device,
        resolution: u32,
        count: usize,
    ) -> Result<&[(Texture, u32)], RenderError> {
        if self.resolution != resolution || self.maps.len() != count {
            self.destroy(device);
            let side = NonZeroU32::new(resolution).unwrap_or(NonZeroU32::MIN);
            for i in 0..count {
                let texture = device.create_texture(&TextureDesc {
                    width: side,
                    height: side,
                    mip_levels: NonZeroU32::MIN,
                    samples: SampleCount::X1,
                    format: DEPTH,
                    usage: TextureUsage::RENDER_TARGET
                        | TextureUsage::SAMPLED
                        | TextureUsage::BINDLESS,
                    label: format!("shadow cascade {i}"),
                })?;
                self.maps.push((texture, device.texture_index(texture)?.0));
            }
            self.resolution = resolution;
        }
        Ok(&self.maps)
    }

    pub(crate) fn destroy(&mut self, device: &dyn Device) {
        self.maps
            .drain(..)
            .for_each(|(t, _)| device.destroy_texture(t));
        self.resolution = 0;
    }
}

/// Push constants of a shadow draw (mirrors `ShadowPush` in `forward.slang`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShadowPush {
    /// Frame block buffer.
    pub frame: u32,
    /// Instance buffer.
    pub instances: u32,
    /// Instance index.
    pub instance: u32,
    /// Index stream.
    pub indices: u32,
    /// Position stream.
    pub positions: u32,
    /// Cascade.
    pub cascade: u32,
    /// Joint index stream (read only by skinned instances).
    pub joints: u32,
    /// Joint weight stream (read only by skinned instances).
    pub weights: u32,
}

concordia_shaders::shader_struct!(ShadowPush {
    frame,
    instances,
    instance,
    indices,
    positions,
    cascade,
    joints,
    weights
});

impl ShadowPush {
    pub(crate) fn bytes(&self) -> Vec<u8> {
        [
            self.frame,
            self.instances,
            self.instance,
            self.indices,
            self.positions,
            self.cascade,
            self.joints,
            self.weights,
        ]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect()
    }
}

/// A shadow caster: its push constants (cascade filled per pass) and vertex count.
#[derive(Clone)]
pub(crate) struct ShadowDraw {
    pub(crate) push: ShadowPush,
    pub(crate) vertices: u32,
    /// Consecutive instances drawn by this call (same mesh).
    pub(crate) instances: u32,
}

/// The shadow work of one frame.
pub(crate) struct ShadowJob {
    pub(crate) maps: Vec<(Texture, u32)>,
    pub(crate) resolution: u32,
    pub(crate) pipeline: Pipeline,
    pub(crate) casters: Vec<ShadowDraw>,
    /// The meshlet casters' bucket per cascade (culled for that cascade's view).
    pub(crate) meshlets: Vec<crate::meshlet_pass::BucketDraw>,
    /// The maps already hold exactly these shadows: sample them, draw nothing.
    pub(crate) reuse: bool,
}

/// One depth pass per cascade; returns the graph textures the forward pass samples.
pub(crate) fn add_shadow_passes(graph: &mut RenderGraph, job: ShadowJob) -> Vec<GraphTexture> {
    let side = NonZeroU32::new(job.resolution).unwrap_or(NonZeroU32::MIN);
    let casters = std::sync::Arc::new(job.casters);
    let meshlets = std::sync::Arc::new(job.meshlets);
    let mut textures = Vec::with_capacity(job.maps.len());
    for (cascade, (texture, _)) in job.maps.iter().enumerate() {
        let desc = TextureDesc {
            width: side,
            height: side,
            mip_levels: NonZeroU32::MIN,
            samples: SampleCount::X1,
            format: DEPTH,
            usage: TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED,
            label: String::new(),
        };
        let before = if job.reuse {
            Layout::ShaderRead
        } else {
            Layout::Undefined
        };
        let map = graph.import_texture(&format!("shadow {cascade}"), *texture, desc, before, None);
        if job.reuse {
            textures.push(map);
            continue;
        }
        let (casters, meshlets, pipeline) = (casters.clone(), meshlets.clone(), job.pipeline);
        graph.add_pass_recorded(
            &format!("shadow {cascade}"),
            |p| p.uses(map, Use::DepthWrite),
            Some(Box::new(move |ctx: &mut PassContext<'_>| {
                ctx.commands.push(Command::BeginRenderPass(RenderPassDesc {
                    colors: Vec::new(),
                    depth: Some(DepthAttachment {
                        resolve: None,
                        texture: ctx.texture(map),
                        load: LoadOp::Clear(ClearValue::Depth(0.0)),
                        store: StoreOp::Store,
                    }),
                    extent: None,
                }));
                ctx.commands.push(Command::SetPipeline(pipeline));
                for draw in casters.iter() {
                    let push = ShadowPush {
                        cascade: cascade as u32,
                        ..draw.push
                    };
                    ctx.commands.push(Command::PushConstants(push.bytes()));
                    ctx.commands.push(Command::Draw {
                        vertices: 0..draw.vertices,
                        instances: 0..draw.instances,
                    });
                }
                if let Some(bucket) = meshlets.get(cascade) {
                    ctx.commands.push(Command::SetPipeline(bucket.pipeline));
                    ctx.commands.commands.extend(bucket.commands());
                }
                ctx.commands.push(Command::EndRenderPass);
                Ok(())
            })),
        );
        textures.push(map);
    }
    textures
}

/// World-space bounds of `boxes` (object AABB, world matrix) pairs, or a unit box when empty.
pub(crate) fn world_bounds(
    boxes: impl Iterator<Item = ((Vec3, Vec3), concordia_core::math::Mat4)>,
) -> (Vec3, Vec3) {
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for ((lo, hi), world) in boxes {
        for i in 0..8 {
            let p = Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            let w = world.transform_point3(p);
            min = min.min(w);
            max = max.max(w);
        }
    }
    if min.x > max.x {
        (Vec3::splat(-1.0), Vec3::ONE)
    } else {
        (min, max)
    }
}
