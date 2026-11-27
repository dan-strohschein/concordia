//! One frame's virtual-shadow-map work as the renderer plans it: the clipmap, the parameter and
//! depth-view blocks, the page buffers, and the graph passes that run them after the prepass.

use std::sync::Arc;

use concordia_core::math::{IVec2, Mat4, Vec3, Vec4};
use concordia_rhi::{Buffer, Device};

use super::cache::{CacheKey, CacheReset, CacheState};
use super::clipmap::{Clipmap, stable_depth_range};
use super::frame::{Footprint, ParamsIn, depth_views, params_rows};
use super::gpu::{PageStore, PageWork, VsmPipelines, VsmPool, WorkShape};
use super::ledger::{CasterLedger, CasterState};
use super::pool::PoolLayout;
use super::settings::VsmSettings;
use crate::RenderError;
use crate::gpu::Uploads;
use crate::graph::{GraphTexture, PassContext, RenderGraph, Use};

/// What a frame's VSM work is planned from.
pub(crate) struct JobIn<'a> {
    pub(crate) settings: &'a VsmSettings,
    /// The sun's travel direction.
    pub(crate) light_dir: Vec3,
    /// The camera's position and (unjittered) view-projection.
    pub(crate) eye: Vec3,
    pub(crate) camera: Mat4,
    pub(crate) footprint: Footprint,
    pub(crate) pcf_radius: u32,
    /// Every caster's world bounds (min, max).
    pub(crate) casters: (Vec3, Vec3),
    /// The view the frame is for (one cached view at a time).
    pub(crate) view: u64,
    /// Every shadow caster this frame, by key (the ledger compares them with last frame's).
    pub(crate) states: Vec<(crate::visibility::VisKey, CasterState)>,
}

/// What virtual shadow maps keep between frames: the pool, the page store, the depth range, each
/// level's last origin and the cache's key.
#[derive(Default)]
pub(crate) struct VsmResources {
    pub(crate) pool: VsmPool,
    pub(crate) store: PageStore,
    pub(crate) depth_range: Option<(f32, f32)>,
    pub(crate) cache: CacheState,
    origins: Vec<IVec2>,
    ledger: CasterLedger<crate::visibility::VisKey>,
}

impl VsmResources {
    /// Frees the GPU buffers.
    pub(crate) fn destroy(&mut self, device: &dyn Device) {
        self.pool.destroy(device);
        self.store.destroy(device);
    }

    /// Forgets the depth range (its settings changed).
    pub(crate) fn forget_depth_range(&mut self) {
        self.depth_range = None;
    }
}

/// One frame's VSM work.
#[derive(Debug, Clone)]
pub(crate) struct VsmJob {
    pub(crate) clipmap: Clipmap,
    pub(crate) work: PageWork,
    /// Bindless index of the parameter block.
    pub(crate) params: u32,
    /// Bindless index of the depth views (the camera, then each level).
    pub(crate) views: u32,
    /// The pool's buffers.
    pub(crate) pool: Vec<Buffer>,
    /// Why the store is cleared before this frame, if it is.
    pub(crate) reset: Option<CacheReset>,
    /// Bindless index and count of the spheres whose pages are stale.
    pub(crate) stale: (u32, u32),
}

impl VsmJob {
    /// Plans the frame: fits the clipmap (keeping `depth_range` while the casters fit in it),
    /// ensures the pool, uploads the blocks and allocates the page buffers (kept in `keep`).
    pub(crate) fn prepare(
        device: &dyn Device,
        up: &mut Uploads,
        pipelines: VsmPipelines,
        input: &JobIn<'_>,
        res: &mut VsmResources,
        keep: &mut Vec<Buffer>,
    ) -> Result<Self, RenderError> {
        let s = input.settings;
        let layout = PoolLayout::new(s.pool_pages)
            .map_err(|e| RenderError::InvalidSettings(format!("[render.vsm] {e}")))?;
        let light_view = crate::shadow::light_rotation(input.light_dir);
        let range = stable_depth_range(light_z_range(&light_view, input.casters), res.depth_range);
        res.depth_range = Some(range);
        let clipmap = Clipmap::fit(input.light_dir, input.eye, s.levels, range);
        let (store, fresh) = res
            .store
            .ensure(device, (u32::from(s.levels), s.pool_pages))?;
        if fresh {
            res.cache.reset(CacheReset::First);
        }
        let key = CacheKey {
            view: input.view,
            light_dir: input.light_dir,
            levels: s.levels,
            pool_pages: s.pool_pages,
            depth: range,
        };
        let reset = res.cache.begin(key, s.cache);
        let mut stale = res.ledger.diff(input.states.iter().cloned());
        if reset.is_some() {
            stale.clear(); // the store starts empty: nothing to invalidate
        }
        let stale_count = stale.len() as u32;
        let rows: Vec<u8> = stale
            .iter()
            .flat_map(|v: &Vec4| v.to_array())
            .flat_map(f32::to_le_bytes)
            .chain([0; 16])
            .collect();
        let stale_buf = up.buffer(device, &rows, "vsm stale spheres")?;
        keep.push(stale_buf);
        let origins: Vec<IVec2> = clipmap.levels.iter().map(|l| l.origin_page()).collect();
        let previous = match (reset, res.origins.len() == origins.len()) {
            (None, true) => std::mem::replace(&mut res.origins, origins.clone()),
            _ => {
                res.origins.clone_from(&origins);
                origins
            }
        };
        let chunks = res.pool.ensure(device, layout)?;
        let indices: Vec<u32> = chunks.iter().map(|(_, i)| *i).collect();
        let pool: Vec<Buffer> = chunks.iter().map(|(b, _)| *b).collect();
        let params = params_rows(&ParamsIn {
            settings: s,
            clipmap: &clipmap,
            eye: input.eye,
            footprint: input.footprint,
            pcf_radius: input.pcf_radius,
            chunks: &indices,
            inv_view_proj: input.camera.inverse(),
            previous: &previous,
        });
        let params = up.buffer(device, &params, "vsm params")?;
        let views = up.buffer(device, &depth_views(&input.camera, &clipmap), "depth views")?;
        keep.extend([params, views]);
        let work = PageWork::prepare(
            device,
            up,
            pipelines,
            WorkShape {
                store,
                levels: u32::from(s.levels),
                pool: s.pool_pages,
                keep_frames: u32::from(s.keep_frames),
            },
            keep,
        )?;
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        Ok(Self {
            clipmap,
            work,
            params: index(params)?,
            views: index(views)?,
            pool,
            reset,
            stale: (index(stale_buf)?, stale_count),
        })
    }
}

/// The light-space z range (far, near) of the box `bounds`.
fn light_z_range(light_view: &Mat4, (lo, hi): (Vec3, Vec3)) -> (f32, f32) {
    (0..8).fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), i| {
        let p = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let z = light_view.transform_point3(p).z;
        (a.min(z), b.max(z))
    })
}

/// `vsm.age`, `vsm.mark` (from the camera's single-sample `depth`, `size` pixels), `vsm.allocate`
/// and `vsm.clear`, in that order after the prepass.
pub(crate) fn add_page_passes(
    graph: &mut RenderGraph,
    job: &VsmJob,
    (depth, size): (GraphTexture, [u32; 2]),
    device: Arc<dyn Device>,
) {
    let begin = job.work.begin(job.reset.is_some(), job.params);
    graph.add_pass_recorded(
        "vsm.age",
        |p| p.side_effect(),
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            ctx.commands.commands.extend(begin.iter().cloned());
            Ok(())
        })),
    );
    let invalidate = job.work.invalidate(job.stale, (job.params, job.views));
    graph.add_pass_recorded(
        "vsm.invalidate",
        |p| p.side_effect(),
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            ctx.commands.commands.extend(invalidate.iter().cloned());
            Ok(())
        })),
    );
    let mark = job.work.clone();
    let (params, views) = (job.params, job.views);
    graph.add_pass_recorded(
        "vsm.mark",
        |p| {
            p.uses(depth, Use::SampledRead);
            p.side_effect();
        },
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            let depth = device.texture_index(ctx.texture(depth))?.0;
            ctx.commands
                .commands
                .extend(mark.mark((depth, size), (params, views)));
            Ok(())
        })),
    );
    let allocate = job.work.allocate();
    graph.add_pass_recorded(
        "vsm.allocate",
        |p| p.side_effect(),
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            ctx.commands.commands.extend(allocate.iter().cloned());
            Ok(())
        })),
    );
    let clear = job.work.clear(job.params, &job.pool);
    graph.add_pass_recorded(
        "vsm.clear",
        |p| p.side_effect(),
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            ctx.commands.commands.extend(clear.iter().cloned());
            Ok(())
        })),
    );
}

/// What draws a frame's VSM casters, one page per pair (M4 4.7a2): the levels' meshlet job (its
/// cull runs as `vsm.cull`), the binning of its survivors and of the M1 casters (`vsm.bin`), and
/// each draw's push constants and arguments (`vsm.render`).
pub(crate) struct CasterDraws {
    pub(crate) cull: Option<crate::meshlet_pass::MeshletJob>,
    pub(crate) meshlet_bins: Option<super::bin::MeshletBins>,
    pub(crate) unit_bins: Option<super::bin::UnitBins>,
    /// The page pipelines (M1 casters, meshlets).
    pub(crate) pipelines: (concordia_rhi::Pipeline, concordia_rhi::Pipeline),
    /// Per M1 caster draw: its push constants (`VsmPagePush`) and vertex count's record.
    pub(crate) m1: Vec<Vec<u8>>,
    /// The meshlet pairs' push constants (`MeshletPagePush`).
    pub(crate) meshlet_push: Vec<u8>,
    /// The pool, written here and read by `forward`.
    pub(crate) pool: Vec<Buffer>,
    /// Slots in the levels' meshlet plan (their cull codes are read back).
    pub(crate) entries: u32,
}

/// `vsm.cull` (compute route), `vsm.bin` and `vsm.render`: a 128 × 128 attachment-less pass in
/// which each pair's caster is drawn into its own page.
pub(crate) fn add_render_pass(graph: &mut RenderGraph, draws: CasterDraws) {
    use concordia_rhi::{Command, RenderPassDesc};
    if let Some(job) = &draws.cull {
        job.add_named_pass(graph, "vsm.cull");
    }
    let mut bin: Vec<Command> = Vec::new();
    if let Some(b) = &draws.meshlet_bins {
        bin.extend_from_slice(b.commands());
    }
    if let Some(b) = &draws.unit_bins {
        bin.extend_from_slice(b.commands());
    }
    graph.add_pass_recorded(
        "vsm.bin",
        |p| p.side_effect(),
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            ctx.commands.commands.extend(bin.iter().cloned());
            Ok(())
        })),
    );
    let side =
        std::num::NonZeroU32::new(super::table::PAGE_TEXELS).unwrap_or(std::num::NonZeroU32::MIN);
    graph.add_pass_recorded(
        "vsm.render",
        |p| p.side_effect(),
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            ctx.commands.push(Command::BeginRenderPass(RenderPassDesc {
                colors: Vec::new(),
                depth: None,
                extent: Some([side, side]),
            }));
            if let Some(units) = &draws.unit_bins {
                ctx.commands.push(Command::SetPipeline(draws.pipelines.0));
                for (d, push) in draws.m1.iter().enumerate() {
                    ctx.commands.push(Command::PushConstants(push.clone()));
                    ctx.commands.push(Command::DrawIndirect {
                        args: units.args,
                        offset: d as u64 * concordia_rhi::DRAW_INDIRECT_BYTES,
                    });
                }
            }
            if let Some(m) = &draws.meshlet_bins {
                ctx.commands.push(Command::SetPipeline(draws.pipelines.1));
                ctx.commands
                    .push(Command::PushConstants(draws.meshlet_push.clone()));
                ctx.commands.push(Command::DrawIndirect {
                    args: m.args,
                    offset: 0,
                });
            }
            ctx.commands.push(Command::EndRenderPass);
            ctx.commands.push(pool_written(&draws.pool));
            Ok(())
        })),
    );
}

/// Fragment writes of the pool before `forward`'s fragment reads.
fn pool_written(pool: &[Buffer]) -> concordia_rhi::Command {
    use concordia_rhi::{Access, AccessFlags, BufferBarrier, Layout, Stages};
    let access = |access| Access {
        stages: Stages::FRAGMENT,
        access,
        layout: Layout::General,
    };
    concordia_rhi::Command::Barrier {
        buffers: pool
            .iter()
            .map(|&buffer| BufferBarrier {
                buffer,
                before: access(AccessFlags::SHADER_WRITE),
                after: access(AccessFlags::SHADER_READ),
            })
            .collect(),
        textures: Vec::new(),
    }
}
