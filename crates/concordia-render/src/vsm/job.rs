//! One frame's virtual-shadow-map work as the renderer plans it: the clipmap, the parameter and
//! depth-view blocks, the page buffers, and the graph passes that run them after the prepass.

use std::sync::Arc;

use concordia_core::math::{IVec2, Mat4, Vec3, Vec4};
use concordia_rhi::{Buffer, Device};

use super::cache::{CacheKey, CacheReset, CacheState};
use super::clipmap::{Clipmap, stable_depth_range};
use super::frame::{Footprint, ParamsIn, depth_views, params_rows};
use super::gpu::{PageStore, PageWork, VsmPipelines, VsmPool, WorkShape};
use super::ledger::{CasterInput, CasterLedger};
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
    pub(crate) states: Vec<(crate::visibility::VisKey, CasterInput)>,
    /// Shadowed spot and point lights in view: entity, light, importance (4.7c).
    pub(crate) locals: Vec<(concordia_core::id::EntityId, super::local::SlotLight, f32)>,
    /// Whether the sun casts (else only local lights mark pages).
    pub(crate) sun: bool,
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
    locals: super::local::LocalSlots,
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
    /// Bindless index and count of the dynamic casters' spheres.
    pub(crate) movers: (u32, u32),
    /// The dynamic casters' keys (the renderer draws them into dynamic pages).
    pub(crate) dynamic: std::collections::BTreeSet<crate::visibility::VisKey>,
    /// `Static`-hinted casters that moved this frame.
    pub(crate) static_hint_moved: Vec<crate::visibility::VisKey>,
    /// Table ranges (first entry, count) released at the frame's start: local lights that changed
    /// or left their slot (4.7c).
    pub(crate) releases: Vec<(u32, u32)>,
    /// Each slot's light (4.7c; its rows carry the slot).
    pub(crate) local_lights: Vec<Option<concordia_core::id::EntityId>>,
    /// Each slot's light as its maps were made.
    pub(crate) local_slots: Vec<Option<super::local::SlotLight>>,
    /// Shadowed local lights beyond the slots.
    pub(crate) local_dropped: usize,
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
        let local = res.locals.update(s.max_local_lights, &input.locals);
        let map_entries = super::local::FACES * super::local::MAP_ENTRIES;
        let sun_entries = u32::from(s.levels) * super::PAGES_PER_SIDE * super::PAGES_PER_SIDE;
        let local_entries = local.slots.len() as u32 * map_entries;
        let (store, fresh) = res
            .store
            .ensure(device, (sun_entries + local_entries, s.pool_pages))?;
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
        let mut verdict = res.ledger.update(input.states.iter().cloned());
        if !s.cache {
            // Uncached, every caster draws into the static pages (4.7a's frame, the oracle).
            verdict.dynamic.clear();
            verdict.dynamic_spheres.clear();
        }
        let mut stale = verdict.stale;
        if reset.is_some() {
            stale.clear(); // the store starts empty: nothing to invalidate
        }
        let stale_count = stale.len() as u32;
        let rows = |spheres: &[Vec4]| -> Vec<u8> {
            spheres
                .iter()
                .flat_map(|v| v.to_array())
                .flat_map(f32::to_le_bytes)
                .chain([0; 16])
                .collect()
        };
        let stale_buf = up.buffer(device, &rows(&stale), "vsm stale spheres")?;
        let movers_buf = up.buffer(
            device,
            &rows(&verdict.dynamic_spheres),
            "vsm dynamic spheres",
        )?;
        keep.extend([stale_buf, movers_buf]);
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
        let work = PageWork::prepare(
            device,
            up,
            pipelines,
            WorkShape {
                store,
                levels: u32::from(s.levels),
                local_entries,
                pool: s.pool_pages,
                keep_frames: u32::from(s.keep_frames),
            },
            keep,
        )?;
        let rows = super::local::local_rows(&local.slots, 2 + u32::from(s.levels), sun_entries);
        let locals = up.buffer(device, &rows, "vsm locals")?;
        keep.push(locals);
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        let params = params_rows(&ParamsIn {
            settings: s,
            clipmap: &clipmap,
            eye: input.eye,
            footprint: input.footprint,
            pcf_radius: input.pcf_radius,
            chunks: &indices,
            inv_view_proj: input.camera.inverse(),
            previous: &previous,
            tables: [index(work.table)?, index(work.dynamic)?, index(locals)?],
            sun: input.sun,
        });
        let params = up.buffer(device, &params, "vsm params")?;
        let local_views = super::local::local_views(&local.slots);
        let views = up.buffer(
            device,
            &depth_views(&input.camera, &clipmap, &local_views),
            "depth views",
        )?;
        keep.extend([params, views]);
        Ok(Self {
            clipmap,
            work,
            params: index(params)?,
            views: index(views)?,
            pool,
            reset,
            stale: (index(stale_buf)?, stale_count),
            movers: (index(movers_buf)?, verdict.dynamic_spheres.len() as u32),
            dynamic: verdict.dynamic.into_iter().collect(),
            static_hint_moved: verdict.static_hint_moved,
            releases: local
                .released
                .iter()
                .map(|k| (sun_entries + k * map_entries, map_entries))
                .collect(),
            local_lights: local.entities,
            local_slots: local.slots.clone(),
            local_dropped: local.dropped,
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
    let begin = job
        .work
        .begin(job.reset.is_some(), job.params, &job.releases);
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
    let allocate = job.work.allocate(job.movers, (job.params, job.views));
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
/// cull runs as `vsm.cull`), then per layer (static, dynamic; 4.7b) the binning of its casters
/// (`vsm.bin`) and each draw's push constants and arguments (`vsm.render`); the composite copies
/// static pages into dynamic ones between the two.
pub(crate) struct CasterDraws {
    pub(crate) cull: Option<crate::meshlet_pass::MeshletJob>,
    /// The page pipelines (M1 casters, meshlets).
    pub(crate) pipelines: (concordia_rhi::Pipeline, concordia_rhi::Pipeline),
    /// Static casters into pages to redraw, then dynamic casters into dynamic (or polluted) pages.
    pub(crate) layers: [CasterLayer; 2],
    /// The composite's dispatch (static → dynamic copies).
    pub(crate) composite: Vec<concordia_rhi::Command>,
    /// The pool, written here and read by `forward`.
    pub(crate) pool: Vec<Buffer>,
    /// Slots in the levels' meshlet plan (their cull codes are read back).
    pub(crate) entries: u32,
}

/// One class of casters' binning and draws.
#[derive(Default)]
pub(crate) struct CasterLayer {
    pub(crate) meshlet_bins: Option<super::bin::MeshletBins>,
    pub(crate) unit_bins: Option<super::bin::UnitBins>,
    /// Per M1 caster draw: its push constants (`VsmPagePush`).
    pub(crate) m1: Vec<Vec<u8>>,
    /// The meshlet pairs' push constants (`MeshletPagePush`).
    pub(crate) meshlet_push: Vec<u8>,
}

impl CasterLayer {
    /// The binning's dispatches.
    fn bin(&self) -> impl Iterator<Item = &concordia_rhi::Command> {
        let meshlets = self.meshlet_bins.iter().flat_map(|b| b.commands());
        meshlets.chain(self.unit_bins.iter().flat_map(|b| b.commands()))
    }

    /// The layer's page draws, inside a 128 × 128 attachment-less pass.
    fn draw(
        &self,
        (m1, meshlets): (concordia_rhi::Pipeline, concordia_rhi::Pipeline),
    ) -> Vec<concordia_rhi::Command> {
        use concordia_rhi::Command;
        let mut out = Vec::new();
        if let Some(units) = &self.unit_bins {
            out.push(Command::SetPipeline(m1));
            for (d, push) in self.m1.iter().enumerate() {
                out.push(Command::PushConstants(push.clone()));
                out.push(Command::DrawIndirect {
                    args: units.args,
                    offset: d as u64 * concordia_rhi::DRAW_INDIRECT_BYTES,
                });
            }
        }
        if let Some(m) = &self.meshlet_bins {
            out.push(Command::SetPipeline(meshlets));
            out.push(Command::PushConstants(self.meshlet_push.clone()));
            out.push(Command::DrawIndirect {
                args: m.args,
                offset: 0,
            });
        }
        out
    }

    /// The pair totals' buffers (for the stats).
    pub(crate) fn totals(&self) -> impl Iterator<Item = Buffer> + '_ {
        let meshlets = self.meshlet_bins.iter().map(|b| b.totals);
        meshlets.chain(self.unit_bins.iter().map(|b| b.totals))
    }
}

/// `vsm.cull` (compute route), `vsm.bin` (both layers) and `vsm.render`: a 128 × 128
/// attachment-less pass of the static layer, the composite, then a pass of the dynamic layer.
pub(crate) fn add_render_pass(graph: &mut RenderGraph, draws: CasterDraws) {
    use concordia_rhi::{Command, RenderPassDesc};
    if let Some(job) = &draws.cull {
        job.add_named_pass(graph, "vsm.cull");
    }
    let bin: Vec<Command> = draws
        .layers
        .iter()
        .flat_map(CasterLayer::bin)
        .cloned()
        .collect();
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
    let pass = move || {
        Command::BeginRenderPass(RenderPassDesc {
            colors: Vec::new(),
            depth: None,
            extent: Some([side, side]),
        })
    };
    graph.add_pass_recorded(
        "vsm.render",
        |p| p.side_effect(),
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            let [statics, dynamics] = &draws.layers;
            ctx.commands.push(pass());
            ctx.commands.commands.extend(statics.draw(draws.pipelines));
            ctx.commands.push(Command::EndRenderPass);
            ctx.commands.push(pool_written(&draws.pool));
            ctx.commands
                .commands
                .extend(draws.composite.iter().cloned());
            ctx.commands.push(pass());
            ctx.commands.commands.extend(dynamics.draw(draws.pipelines));
            ctx.commands.push(Command::EndRenderPass);
            ctx.commands.push(pool_written(&draws.pool));
            Ok(())
        })),
    );
}

/// Fragment writes of the pool before the composite's and `forward`'s reads and the next draws.
fn pool_written(pool: &[Buffer]) -> concordia_rhi::Command {
    use concordia_rhi::{Access, AccessFlags, BufferBarrier, Layout, Stages};
    let access = |stages, access| Access {
        stages,
        access,
        layout: Layout::General,
    };
    concordia_rhi::Command::Barrier {
        buffers: pool
            .iter()
            .map(|&buffer| BufferBarrier {
                buffer,
                before: access(Stages::FRAGMENT, AccessFlags::SHADER_WRITE),
                after: access(
                    Stages::FRAGMENT | Stages::COMPUTE,
                    AccessFlags::SHADER_READ | AccessFlags::SHADER_WRITE,
                ),
            })
            .collect(),
        textures: Vec::new(),
    }
}
