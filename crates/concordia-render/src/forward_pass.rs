//! The forward pass (M1, ADR-0100): the sky, the M1 opaque draws, the meshlet buckets, the blended
//! draws and the grid into (multisampled) HDR, motion and depth, resolved into single-sample HDR and
//! motion. With two-pass occlusion (M4 4.4b) it splits around the HZB: `forward` draws the sky, the
//! M1 opaque draws (occluders) and pass 1's buckets, keeps every target and resolves depth with min;
//! `meshlet.hzb` builds the HZB from that depth and `meshlet.cull_late` culls pass 2 (compute route);
//! `forward_late` loads the targets and draws pass 2's buckets, the blended draws and the grid.

use std::num::NonZeroU32;
use std::sync::Arc;

use concordia_rhi::{
    ClearValue, ColorAttachment, Command, DepthAttachment, DepthResolve, DepthResolveTarget,
    Device, LoadOp, Pipeline, RenderPassDesc, SampleCount, StoreOp, TextureUsage,
};

use crate::CameraView;
use crate::graph::{GraphTexture, PassContext, RenderGraph, Use};
use crate::hzb::Hzb;
use crate::meshlet_pass::{BucketDraw, MeshletJob};
use crate::pipelines::{DEPTH, HDR, MOTION};
use crate::renderer::desc;

/// One draw call, ready to record: `instances` consecutive instances of one mesh and material.
#[derive(Debug, Clone)]
pub(crate) struct Draw {
    pub(crate) pipeline: Pipeline,
    pub(crate) push: Vec<u8>,
    pub(crate) vertices: u32,
    pub(crate) instances: u32,
    /// What may share the call: the pipeline, material and mesh streams.
    pub(crate) batch: (Pipeline, u32, u32),
    pub(crate) first: u32,
    /// Blended (drawn after the meshlet buckets).
    pub(crate) blend: bool,
}

/// A pipeline and its push constants, for a one-call draw (the sky, the grid).
pub(crate) type FullDraw = Option<(Pipeline, Vec<u8>)>;

/// The forward pass's resolved graph textures.
pub(crate) struct ForwardTargets {
    pub(crate) hdr: GraphTexture,
    /// Motion vectors, resolved (M4 4.3b2).
    pub(crate) motion: GraphTexture,
    /// Depth (single-sample and stored when the frame keeps it for a temporal resolve).
    pub(crate) depth: GraphTexture,
}

/// The split's HZB build: the frame's HZB, its pipeline, and the device (for bindless indices);
/// and the `hzb` view's draw, last in pass 2, in that view.
pub(crate) struct Split {
    pub(crate) hzb: Hzb,
    pub(crate) pipeline: Pipeline,
    pub(crate) device: Arc<dyn Device>,
    pub(crate) view: FullDraw,
}

/// What one forward pass draws: the sky, M1 draws (opaque first), buckets, the grid, and a
/// full-screen draw over everything (the `hzb` view).
#[derive(Clone)]
struct Contents {
    sky: FullDraw,
    draws: Vec<Draw>,
    buckets: Vec<BucketDraw>,
    grid: FullDraw,
    over: FullDraw,
}

/// Which part of the frame a forward pass is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// The only one: clears and resolves.
    Whole,
    /// Pass 1 of a split: clears, keeps every target, resolves depth with min.
    Early,
    /// Pass 2 of a split: loads every target and resolves colour.
    Late,
}

/// The graph textures a forward pass draws into.
#[derive(Debug, Clone, Copy)]
struct Targets {
    hdr: GraphTexture,
    motion: GraphTexture,
    depth: GraphTexture,
    /// Multisampled colour and motion (with MSAA).
    msaa: Option<(GraphTexture, GraphTexture)>,
    /// Pass 1's min-resolved depth (a split with MSAA).
    resolved: Option<GraphTexture>,
    /// Store the depth after the last pass, sampled (a temporal resolve reads it; M4 4.6).
    keep_depth: bool,
}

/// Adds the forward pass (or, with `split`, its two halves, the HZB build and pass 2's culling).
pub(crate) fn add_forward(
    graph: &mut RenderGraph,
    view: &CameraView,
    (draws, meshlets): (Vec<Draw>, Option<MeshletJob>),
    (samples, size): (SampleCount, (NonZeroU32, NonZeroU32)),
    shadow_maps: &[GraphTexture],
    (sky, grid): (FullDraw, FullDraw),
    (split, keep_depth): (Option<Split>, bool),
) -> ForwardTargets {
    let targets = create_targets(graph, samples, size, (split.is_some(), keep_depth));
    // HDR alpha is the reactive mask (M4 4.6): 0 under the sky and opaque surfaces, blended
    // coverage over them. Nothing else reads it.
    let [r, g, b, _] = view.camera.clear;
    let clear = [r, g, b, 0.0];
    let (forward, late) = match &meshlets {
        Some(m) => (m.forward.clone(), m.forward_late.clone()),
        None => (Vec::new(), Vec::new()),
    };
    match split {
        None => {
            let all = Contents {
                sky,
                draws,
                buckets: forward,
                grid,
                over: None,
            };
            add_part(
                graph,
                "forward",
                Part::Whole,
                (targets, clear),
                shadow_maps,
                all,
            );
        }
        Some(split) => {
            let opaque = draws.iter().take_while(|d| !d.blend).count();
            let early = Contents {
                sky,
                draws: draws[..opaque].to_vec(),
                buckets: forward,
                grid: None,
                over: None,
            };
            add_part(
                graph,
                "forward",
                Part::Early,
                (targets, clear),
                shadow_maps,
                early,
            );
            let view = split.view.clone();
            add_hzb(graph, targets.resolved.unwrap_or(targets.depth), split);
            if let Some(m) = &meshlets {
                m.add_late_pass(graph);
            }
            let rest = Contents {
                sky: None,
                draws: draws[opaque..].to_vec(),
                buckets: late,
                grid,
                over: view,
            };
            add_part(
                graph,
                "forward_late",
                Part::Late,
                (targets, clear),
                shadow_maps,
                rest,
            );
        }
    }
    ForwardTargets {
        hdr: targets.hdr,
        motion: targets.motion,
        depth: targets.depth,
    }
}

/// The pass's targets; a split's HZB source (the depth, or its min resolve with MSAA) is sampled.
fn create_targets(
    graph: &mut RenderGraph,
    samples: SampleCount,
    size: (NonZeroU32, NonZeroU32),
    (split, keep_depth): (bool, bool),
) -> Targets {
    debug_assert!(
        !keep_depth || samples == SampleCount::X1,
        "kept depth is single-sample"
    );
    let sampled = TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED | TextureUsage::BINDLESS;
    let multisampled = samples != SampleCount::X1;
    let hdr = graph.create_texture(desc(size, HDR, SampleCount::X1, sampled, "hdr"));
    let depth_usage = match (split || keep_depth) && !multisampled {
        true => sampled,
        false => TextureUsage::RENDER_TARGET,
    };
    let depth = graph.create_texture(desc(size, DEPTH, samples, depth_usage, "depth"));
    let motion = graph.create_texture(desc(
        size,
        MOTION,
        SampleCount::X1,
        sampled | TextureUsage::COPY_SRC,
        "motion",
    ));
    let msaa = multisampled.then(|| {
        let rt = TextureUsage::RENDER_TARGET;
        (
            graph.create_texture(desc(size, HDR, samples, rt, "hdr_msaa")),
            graph.create_texture(desc(size, MOTION, samples, rt, "motion_msaa")),
        )
    });
    let resolved = (split && multisampled).then(|| {
        graph.create_texture(desc(
            size,
            DEPTH,
            SampleCount::X1,
            sampled,
            "depth_resolved",
        ))
    });
    Targets {
        hdr,
        motion,
        depth,
        msaa,
        resolved,
        keep_depth,
    }
}

/// One forward pass of `part`, drawing `contents` into `targets`.
fn add_part(
    graph: &mut RenderGraph,
    name: &str,
    part: Part,
    (targets, clear): (Targets, [f32; 4]),
    shadow_maps: &[GraphTexture],
    contents: Contents,
) {
    let shadow_maps = shadow_maps.to_vec();
    graph.add_pass_recorded(
        name,
        |p| {
            p.uses(targets.depth, Use::DepthWrite);
            match targets.msaa {
                Some((hdr, motion)) => {
                    p.uses(hdr, Use::ColorWrite);
                    p.uses(motion, Use::ColorWrite);
                    if part != Part::Early {
                        p.uses(targets.hdr, Use::ColorWrite);
                        p.uses(targets.motion, Use::ColorWrite);
                    }
                }
                None => {
                    p.uses(targets.hdr, Use::ColorWrite);
                    p.uses(targets.motion, Use::ColorWrite);
                }
            }
            if let (Part::Early, Some(resolved)) = (part, targets.resolved) {
                p.uses(resolved, Use::DepthWrite);
            }
            for map in &shadow_maps {
                p.uses(*map, Use::SampledRead);
            }
            // Pass 2 loads what pass 1 keeps, which the graph doesn't see as a read.
            if part == Part::Early {
                p.side_effect();
            }
        },
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            ctx.commands.push(Command::BeginRenderPass(pass_desc(
                ctx, part, targets, clear,
            )));
            record(&contents, &mut ctx.commands.commands);
            ctx.commands.push(Command::EndRenderPass);
            Ok(())
        })),
    );
}

/// The attachments of a forward pass of `part`.
fn pass_desc(ctx: &PassContext<'_>, part: Part, t: Targets, clear: [f32; 4]) -> RenderPassDesc {
    let (load, depth_load) = match part {
        Part::Late => (LoadOp::Load, LoadOp::Load),
        _ => (
            LoadOp::Clear(ClearValue::Color(clear)),
            LoadOp::Clear(ClearValue::Depth(0.0)),
        ),
    };
    let color = |single: GraphTexture, msaa: Option<GraphTexture>, load| match msaa {
        Some(m) if part == Part::Early => ColorAttachment {
            texture: ctx.texture(m),
            resolve: None,
            load,
            store: StoreOp::Store,
        },
        Some(m) => ColorAttachment {
            texture: ctx.texture(m),
            resolve: Some(ctx.texture(single)),
            load,
            store: StoreOp::DontCare,
        },
        None => ColorAttachment {
            texture: ctx.texture(single),
            resolve: None,
            load,
            store: StoreOp::Store,
        },
    };
    let motion_load = match part {
        Part::Late => LoadOp::Load,
        _ => LoadOp::Clear(ClearValue::Color([0.0; 4])),
    };
    RenderPassDesc {
        colors: vec![
            color(t.hdr, t.msaa.map(|m| m.0), load),
            color(t.motion, t.msaa.map(|m| m.1), motion_load),
        ],
        depth: Some(DepthAttachment {
            texture: ctx.texture(t.depth),
            resolve: match (part, t.resolved) {
                (Part::Early, Some(r)) => Some(DepthResolveTarget {
                    texture: ctx.texture(r),
                    filter: DepthResolve::Min,
                }),
                _ => None,
            },
            load: depth_load,
            store: match (part, t.keep_depth) {
                (Part::Early, _) | (_, true) => StoreOp::Store,
                _ => StoreOp::DontCare,
            },
        }),
    }
}

/// Records `c` in order: the sky, the opaque M1 draws, the buckets, the blended draws, the grid,
/// the draw over everything.
fn record(c: &Contents, commands: &mut Vec<Command>) {
    let full = |draw: &FullDraw, vertices: u32, commands: &mut Vec<Command>| {
        if let Some((pipeline, push)) = draw {
            commands.extend([
                Command::SetPipeline(*pipeline),
                Command::PushConstants(push.clone()),
                Command::Draw {
                    vertices: 0..vertices,
                    instances: 0..1,
                },
            ]);
        }
    };
    full(&c.sky, 3, commands);
    let mut bound = None;
    let opaque = c.draws.iter().take_while(|d| !d.blend).count();
    for (i, draw) in c.draws.iter().enumerate() {
        if i == opaque {
            bound = draw_buckets(&c.buckets, commands, bound);
        }
        if bound != Some(draw.pipeline) {
            commands.push(Command::SetPipeline(draw.pipeline));
            bound = Some(draw.pipeline);
        }
        commands.push(Command::PushConstants(draw.push.clone()));
        commands.push(Command::Draw {
            vertices: 0..draw.vertices,
            instances: 0..draw.instances,
        });
    }
    if opaque == c.draws.len() {
        draw_buckets(&c.buckets, commands, bound);
    }
    full(&c.grid, 6, commands);
    full(&c.over, 3, commands);
}

/// Records the buckets' draws; returns the pipeline left bound.
fn draw_buckets(
    buckets: &[BucketDraw],
    commands: &mut Vec<Command>,
    mut bound: Option<Pipeline>,
) -> Option<Pipeline> {
    for bucket in buckets {
        if bound != Some(bucket.pipeline) {
            commands.push(Command::SetPipeline(bucket.pipeline));
            bound = Some(bucket.pipeline);
        }
        commands.extend(bucket.commands());
    }
    bound
}

/// The `meshlet.hzb` pass: the HZB from pass 1's single-sample depth `source`.
fn add_hzb(graph: &mut RenderGraph, source: GraphTexture, split: Split) {
    graph.add_pass_recorded(
        "meshlet.hzb",
        |p| {
            p.uses(source, Use::SampledRead);
            p.side_effect();
        },
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            let depth = split.device.texture_index(ctx.texture(source))?.0;
            let hzb = split.device.buffer_index(split.hzb.buffer)?.0;
            ctx.commands
                .commands
                .extend(split.hzb.commands(split.pipeline, (depth, hzb)));
            Ok(())
        })),
    );
}
