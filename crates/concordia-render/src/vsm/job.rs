//! One frame's virtual-shadow-map work as the renderer plans it: the clipmap, the parameter and
//! depth-view blocks, the page buffers, and the graph passes that run them after the prepass.

use std::sync::Arc;

use concordia_core::math::{Mat4, Vec3};
use concordia_rhi::{Buffer, Device};

use super::clipmap::{Clipmap, stable_depth_range};
use super::frame::{Footprint, ParamsIn, depth_views, params_rows};
use super::gpu::{PageWork, VsmPipelines, VsmPool};
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
}

impl VsmJob {
    /// Plans the frame: fits the clipmap (keeping `depth_range` while the casters fit in it),
    /// ensures the pool, uploads the blocks and allocates the page buffers (kept in `keep`).
    pub(crate) fn prepare(
        device: &dyn Device,
        up: &mut Uploads,
        pipelines: VsmPipelines,
        input: &JobIn<'_>,
        (pool, depth_range): (&mut VsmPool, &mut Option<(f32, f32)>),
        keep: &mut Vec<Buffer>,
    ) -> Result<Self, RenderError> {
        let s = input.settings;
        let layout = PoolLayout::new(s.pool_pages)
            .map_err(|e| RenderError::InvalidSettings(format!("[render.vsm] {e}")))?;
        let light_view = crate::shadow::light_rotation(input.light_dir);
        let range = stable_depth_range(light_z_range(&light_view, input.casters), *depth_range);
        *depth_range = Some(range);
        let clipmap = Clipmap::fit(input.light_dir, input.eye, s.levels, range);
        let chunks = pool.ensure(device, layout)?;
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
        });
        let params = up.buffer(device, &params, "vsm params")?;
        let views = up.buffer(device, &depth_views(&input.camera, &clipmap), "depth views")?;
        keep.extend([params, views]);
        let work = PageWork::prepare(
            device,
            up,
            pipelines,
            (u32::from(s.levels), s.pool_pages),
            keep,
        )?;
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        Ok(Self {
            clipmap,
            work,
            params: index(params)?,
            views: index(views)?,
            pool,
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

/// `vsm.mark` (from the camera's single-sample `depth`, `size` pixels), `vsm.allocate` and
/// `vsm.clear`, in that order after the prepass.
pub(crate) fn add_page_passes(
    graph: &mut RenderGraph,
    job: &VsmJob,
    (depth, size): (GraphTexture, [u32; 2]),
    device: Arc<dyn Device>,
) {
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
