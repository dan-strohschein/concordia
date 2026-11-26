//! GPU side of the meshlet path (ADR-0100): the frame's meshlet buffers and each bucket's draw,
//! by the compute + indirect route (the `meshlet.cull` pass: cull, scan and compact, M4 4.3a and
//! 4.4) or the mesh-shader route (one mesh-task draw per bucket that culls in its object stage,
//! 4.3b and 4.4). Both routes write one cull code per entry.

use std::num::NonZeroU64;

use concordia_rhi::{
    Access, AccessFlags, Buffer, BufferBarrier, BufferDesc, BufferUsage, Command, Device, Layout,
    MemoryLocation, Pipeline, Stages,
};

use crate::RenderError;
use crate::frame::push_rows_u32;
use crate::gpu::Uploads;
use crate::graph::{PassContext, RenderGraph};
use crate::meshlet::{
    CompactPush, CullPush, MeshletPlan, MeshletPush, Route, TaskPush, task_groups,
};
use crate::scan::{Scan, ScanPipelines, compute_barrier};

/// A forward bucket's key: its pipeline and its material's first row.
pub(crate) type BucketKey = (Pipeline, u32);

/// Groups of 64 threads per dispatch row (the most a dimension allows everywhere).
const ROW_GROUPS: u32 = 65_535;

/// The dispatch size covering `entries` threads (64 per group, rows of [`ROW_GROUPS`] groups).
pub(crate) fn cull_groups(entries: u32) -> [u32; 3] {
    let groups = entries.div_ceil(64);
    match groups <= ROW_GROUPS {
        true => [groups, 1, 1],
        false => [ROW_GROUPS, groups.div_ceil(ROW_GROUPS), 1],
    }
}

/// Bindless indices of the frame's tables the meshlet shaders read.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameTables {
    pub(crate) frame: u32,
    pub(crate) instances: u32,
    pub(crate) materials: u32,
    pub(crate) meshes: u32,
    /// The cull view table (`cull::CullView::rows`, camera first).
    pub(crate) views: u32,
}

/// The compute route's culling pipelines.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CullPipelines {
    pub(crate) cull: Pipeline,
    pub(crate) compact: Pipeline,
    pub(crate) scan: ScanPipelines,
}

/// The pipelines a route draws with: forward pipelines come with the buckets' keys.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RoutePipelines {
    /// `meshlet.cull` and the vertex-shader shadow pipeline.
    ComputeIndirect {
        cull: CullPipelines,
        shadow: Pipeline,
    },
    /// The mesh-shader shadow pipeline.
    MeshTasks { shadow: Pipeline },
}

/// One bucket's draw, ready to record: its pipeline, push constants and draw command.
#[derive(Debug, Clone)]
pub(crate) struct BucketDraw {
    pub(crate) pipeline: Pipeline,
    push: Vec<u8>,
    draw: Command,
}

impl BucketDraw {
    /// The push constants and draw (the pipeline is set by the caller).
    pub(crate) fn commands(&self) -> [Command; 2] {
        [Command::PushConstants(self.push.clone()), self.draw.clone()]
    }
}

/// The `meshlet.cull` pass: code every entry, sum the survivors, pack them and write arguments.
#[derive(Debug, Clone)]
struct Compute {
    cull: (Pipeline, CullPush),
    compact: (Pipeline, CompactPush),
    groups: [u32; 3],
    scan: Scan,
    codes: Buffer,
    drawn: Buffer,
    entries: Buffer,
    /// One `DrawIndirect` record per bucket.
    args: Buffer,
}

/// The frame's meshlet work.
#[derive(Debug, Clone)]
pub(crate) struct MeshletJob {
    /// How the buckets reach the rasterizer.
    pub(crate) route: Route,
    /// The compute route's cull pass.
    compute: Option<Compute>,
    /// Forward buckets, in plan order.
    pub(crate) forward: Vec<BucketDraw>,
    /// The shadow casters' bucket per cascade, in cascade order.
    pub(crate) shadows: Vec<BucketDraw>,
    /// One cull code per entry, written by either route.
    pub(crate) codes: Buffer,
}

/// A GPU-only storage buffer of `bytes` bytes.
fn gpu_buffer(
    device: &dyn Device,
    up: &Uploads,
    bytes: u64,
    usage: BufferUsage,
    label: &str,
) -> Result<Buffer, RenderError> {
    Ok(up.alloc(
        device,
        &BufferDesc {
            size: NonZeroU64::new(bytes).unwrap_or(NonZeroU64::MIN),
            usage: usage | BufferUsage::STORAGE | BufferUsage::BINDLESS | BufferUsage::COPY_SRC,
            memory: MemoryLocation::GpuOnly,
            label: label.into(),
        },
    )?)
}

impl MeshletJob {
    /// Uploads `plan`'s work items and bucket table and allocates the codes (and, on the compute
    /// route, the cull pass's buffers); `None` when nothing takes the path. Every buffer is
    /// returned in `keep`.
    pub(crate) fn prepare(
        device: &dyn Device,
        up: &mut Uploads,
        plan: &MeshletPlan<BucketKey>,
        (tables, pipelines): (FrameTables, RoutePipelines),
        keep: &mut Vec<Buffer>,
    ) -> Result<Option<Self>, RenderError> {
        if plan.entries == 0 {
            return Ok(None);
        }
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        let mut items = Vec::new();
        push_rows_u32(&mut items, &plan.items);
        let items = up.buffer(device, &items, "meshlet work items")?;
        let mut buckets = Vec::new();
        let rows: Vec<[u32; 4]> = plan
            .buckets
            .iter()
            .map(|b| [b.first, b.entries, b.vertices, b.cull_word()])
            .collect();
        push_rows_u32(&mut buckets, &rows);
        let buckets = up.buffer(device, &buckets, "meshlet buckets")?;
        let codes = gpu_buffer(
            device,
            up,
            u64::from(plan.entries) * 4,
            BufferUsage::empty(),
            "meshlet cull codes",
        )?;
        keep.extend([items, buckets, codes]);
        let (items, buckets_i, codes_i) = (index(items)?, index(buckets)?, index(codes)?);
        let (route, compute, shadow_pipeline) = match pipelines {
            RoutePipelines::ComputeIndirect { cull, shadow } => {
                let refs = (items, buckets_i, codes);
                let job = Compute::prepare(device, up, plan, (tables, refs, cull), keep)?;
                (Route::ComputeIndirect, Some(job), shadow)
            }
            RoutePipelines::MeshTasks { shadow } => (Route::MeshTasks, None, shadow),
        };
        let draw = |pipeline, material, i: usize| -> Result<BucketDraw, RenderError> {
            let bucket = &plan.buckets[i];
            let cascade = bucket.view.saturating_sub(1);
            Ok(match &compute {
                Some(c) => BucketDraw {
                    pipeline,
                    push: MeshletPush {
                        frame: tables.frame,
                        instances: tables.instances,
                        entries: index(c.entries)?,
                        materials: tables.materials,
                        material,
                        meshes: tables.meshes,
                        first: bucket.first,
                        cascade,
                    }
                    .bytes(),
                    draw: Command::DrawIndirect {
                        args: c.args,
                        offset: i as u64 * concordia_rhi::DRAW_INDIRECT_BYTES,
                    },
                },
                None => BucketDraw {
                    pipeline,
                    push: TaskPush {
                        frame: tables.frame,
                        instances: tables.instances,
                        items,
                        materials: tables.materials,
                        material,
                        meshes: tables.meshes,
                        first: bucket.first,
                        count: bucket.entries,
                        item_count: plan.items.len() as u32,
                        views: tables.views,
                        codes: codes_i,
                        cull: bucket.cull_word(),
                        cascade,
                    }
                    .bytes(),
                    draw: Command::DrawMeshTasks {
                        groups: task_groups(bucket.entries),
                    },
                },
            })
        };
        let mut forward = Vec::new();
        let mut shadows = Vec::new();
        for (i, bucket) in plan.buckets.iter().enumerate() {
            match bucket.key {
                Some((pipeline, material)) => forward.push(draw(pipeline, material, i)?),
                None => shadows.push(draw(shadow_pipeline, 0, i)?),
            }
        }
        Ok(Some(Self {
            route,
            compute,
            forward,
            shadows,
            codes,
        }))
    }

    /// The `meshlet.cull` pass (compute route only).
    pub(crate) fn add_pass(&self, graph: &mut RenderGraph) {
        let Some(compute) = self.compute.clone() else {
            return;
        };
        graph.add_pass_recorded(
            "meshlet.cull",
            |p| p.side_effect(),
            Some(Box::new(move |ctx: &mut PassContext<'_>| {
                ctx.commands.commands.extend(compute.commands());
                Ok(())
            })),
        );
    }
}

impl Compute {
    /// Allocates the drawn flags, their sums, the entries and the arguments, and prepares the
    /// scan; `refs` are the work items' and bucket table's bindless indices and the code buffer.
    fn prepare(
        device: &dyn Device,
        up: &Uploads,
        plan: &MeshletPlan<BucketKey>,
        (tables, refs, pipelines): (FrameTables, (u32, u32, Buffer), CullPipelines),
        keep: &mut Vec<Buffer>,
    ) -> Result<Self, RenderError> {
        let (items, buckets, codes) = refs;
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        let total = plan.entries;
        let words = |n: u32| u64::from(n) * 4;
        let drawn = gpu_buffer(
            device,
            up,
            words(total),
            BufferUsage::empty(),
            "meshlet drawn",
        )?;
        let sums = gpu_buffer(
            device,
            up,
            words(total + 1),
            BufferUsage::empty(),
            "meshlet sums",
        )?;
        let entries = gpu_buffer(
            device,
            up,
            u64::from(total) * 8,
            BufferUsage::empty(),
            "meshlet entries",
        )?;
        let args = gpu_buffer(
            device,
            up,
            plan.buckets.len() as u64 * concordia_rhi::DRAW_INDIRECT_BYTES,
            BufferUsage::INDIRECT,
            "meshlet arguments",
        )?;
        keep.extend([drawn, sums, entries, args]);
        let scan = Scan::prepare(device, up, pipelines.scan, (drawn, sums), total, keep)?;
        let item_count = plan.items.len() as u32;
        Ok(Self {
            cull: (
                pipelines.cull,
                CullPush {
                    items,
                    item_count,
                    buckets,
                    views: tables.views,
                    instances: tables.instances,
                    meshes: tables.meshes,
                    codes: index(codes)?,
                    drawn: index(drawn)?,
                    total,
                },
            ),
            compact: (
                pipelines.compact,
                CompactPush {
                    items,
                    item_count,
                    buckets,
                    drawn: index(drawn)?,
                    sums: index(sums)?,
                    entries: index(entries)?,
                    args: index(args)?,
                    total,
                },
            ),
            groups: cull_groups(total),
            scan,
            codes,
            drawn,
            entries,
            args,
        })
    }

    /// The pass's commands: cull, the scan, compact, and the barrier before the draws.
    fn commands(&self) -> Vec<Command> {
        let mut out = vec![
            Command::SetPipeline(self.cull.0),
            Command::PushConstants(self.cull.1.bytes()),
            Command::Dispatch {
                groups: self.groups,
            },
            compute_barrier(&[self.codes, self.drawn]),
        ];
        out.extend(self.scan.commands().iter().cloned());
        out.extend([
            Command::SetPipeline(self.compact.0),
            Command::PushConstants(self.compact.1.bytes()),
            Command::Dispatch {
                groups: self.groups,
            },
            self.barrier(),
        ]);
        out
    }

    /// Entry and argument writes before the vertex shaders and indirect draws read them.
    fn barrier(&self) -> Command {
        let written = Access {
            stages: Stages::COMPUTE,
            access: AccessFlags::SHADER_WRITE,
            layout: Layout::General,
        };
        let read = Access {
            stages: Stages::DRAW_INDIRECT | Stages::VERTEX,
            access: AccessFlags::INDIRECT_READ | AccessFlags::SHADER_READ,
            layout: Layout::General,
        };
        Command::Barrier {
            buffers: [self.entries, self.args]
                .into_iter()
                .map(|buffer| BufferBarrier {
                    buffer,
                    before: written,
                    after: read,
                })
                .collect(),
            textures: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cull::{CullCode, CullView, Culling};
    use crate::meshlet::{MeshletInfo, PathDraw, plan};
    use crate::test_gpu::{self, download, run_commands};
    use concordia_core::math::{Mat4, Vec3};

    /// The compute route's pipelines on `device`.
    fn cull_pipelines(device: &dyn Device) -> CullPipelines {
        CullPipelines {
            cull: test_gpu::compute(device, "meshlet.slang", "meshlet_cull"),
            compact: test_gpu::compute(device, "meshlet.slang", "meshlet_compact"),
            scan: ScanPipelines {
                groups: test_gpu::compute(device, "scan.slang", "scan_groups"),
                add: test_gpu::compute(device, "scan.slang", "scan_add"),
            },
        }
    }

    /// Instances at the origin (identity rows) of one mesh row whose blob has `meshlets` meshlets.
    fn tables(
        device: &dyn Device,
        up: &mut Uploads,
        (models, views): (&[Mat4], &[CullView]),
        keep: &mut Vec<Buffer>,
    ) -> FrameTables {
        let blob = crate::meshlet::tests::grid_blob(16);
        let info = MeshletInfo::of(&blob).unwrap().unwrap();
        let blob = up.buffer(device, &blob, "blob").unwrap();
        let streams = crate::meshlet::Streams {
            positions: 0,
            normals: 0,
            tangents: 0,
            uvs: 0,
        };
        let bounds = (Vec3::splat(-1.0), Vec3::splat(17.0));
        let mut meshes = Vec::new();
        let row =
            crate::meshlet::mesh_row(device.buffer_index(blob).unwrap().0, streams, info, bounds);
        push_rows_u32(&mut meshes, &row);
        let mut rows = Vec::new();
        for &model in models {
            crate::frame::InstanceRow {
                model,
                previous: model,
                skin: [0, 0, 0, 1],
                motion: [0; 4],
            }
            .write(&mut rows);
        }
        let mut table = Vec::new();
        views
            .iter()
            .for_each(|v| push_rows_u32(&mut table, &v.rows()));
        let views = table;
        let buffers = [
            up.buffer(device, &meshes, "meshes").unwrap(),
            up.buffer(device, &rows, "instances").unwrap(),
            up.buffer(device, &views, "views").unwrap(),
        ];
        keep.extend([blob]);
        keep.extend(buffers);
        let index = |b| device.buffer_index(b).unwrap().0;
        FrameTables {
            frame: 0,
            instances: index(buffers[1]),
            materials: 0,
            meshes: index(buffers[0]),
            views: index(buffers[2]),
        }
    }

    #[test]
    fn culling_off_writes_every_entry_and_counts_them() {
        let Some(device) = test_gpu::device() else {
            return;
        };
        let device = device.as_ref();
        let meshlets = MeshletInfo::of(&crate::meshlet::tests::grid_blob(16))
            .unwrap()
            .unwrap()
            .meshlets;
        let key = |m| Some((Pipeline::from_raw(0, 0), m));
        let path = |instance, forward, casts| PathDraw {
            instance,
            mesh_row: 0,
            info: MeshletInfo {
                meshlets,
                max_triangles: 128,
            },
            forward,
            casts,
            double_sided: false,
        };
        // Two forward buckets (two instances, then one), then the casters for two cascades.
        let draws = [
            path(0, key(9), false),
            path(1, key(2), true),
            path(2, key(9), true),
        ];
        let plan = plan(&draws, 2);
        let mut up = Uploads::new();
        let mut keep = Vec::new();
        let off = CullView::camera(Mat4::IDENTITY, Vec3::ZERO, true, Culling::OFF);
        let models = [Mat4::IDENTITY; 3];
        let tables = tables(device, &mut up, (&models, &[off; 3]), &mut keep);
        let pipelines = RoutePipelines::ComputeIndirect {
            cull: cull_pipelines(device),
            shadow: Pipeline::from_raw(0, 0),
        };
        let job = MeshletJob::prepare(device, &mut up, &plan, (tables, pipelines), &mut keep)
            .unwrap()
            .unwrap();
        up.flush(device).unwrap();
        assert_eq!(job.shadows.len(), 2, "one shadow draw per cascade");
        let compute = job.compute.expect("the compute route culls");
        run_commands(device, compute.commands());
        let entries = download(device, compute.entries, u64::from(plan.entries) * 8);
        let mut want = Vec::new();
        for item in &plan.items {
            for m in 0..meshlets {
                want.extend([item[0], m]);
            }
        }
        assert_eq!(entries, want, "every (instance, meshlet), in slot order");
        let codes = download(device, job.codes, u64::from(plan.entries) * 4);
        let mesh =
            concordia_cluster::MeshletMesh::decode(&crate::meshlet::tests::grid_blob(16)).unwrap();
        for (slot, &word) in codes.iter().enumerate() {
            let meshlet = slot as u32
                - plan
                    .items
                    .iter()
                    .rev()
                    .find(|i| i[2] <= slot as u32)
                    .unwrap()[2];
            let triangles = u32::from(mesh.meshlets[meshlet as usize].triangle_count);
            assert_eq!(word, CullCode::Drawn as u32 | triangles << 8, "slot {slot}");
        }
        let args = download(device, compute.args, plan.buckets.len() as u64 * 16);
        let want_args: Vec<u32> = plan
            .buckets
            .iter()
            .flat_map(|b| [b.vertices, b.entries, 0, 0])
            .collect();
        assert_eq!(args, want_args, "each bucket's draw arguments");
    }

    /// Survivors are packed in slot order into their bucket's region, and each bucket's draw
    /// counts exactly its survivors.
    #[test]
    fn survivors_keep_slot_order() {
        use concordia_core::math::camera::rh::proj::directx;
        let Some(device) = test_gpu::device() else {
            return;
        };
        let device = device.as_ref();
        let meshlets = MeshletInfo::of(&crate::meshlet::tests::grid_blob(16))
            .unwrap()
            .unwrap()
            .meshlets;
        let key = |m| Some((Pipeline::from_raw(0, 0), m));
        let path = |instance, forward| PathDraw {
            instance,
            mesh_row: 0,
            info: MeshletInfo {
                meshlets,
                max_triangles: 128,
            },
            forward,
            casts: false,
            double_sided: false,
        };
        // A narrow view of part of the grid: the second instance is far aside, the others are cut.
        let eye = Vec3::new(4.0, 4.0, 10.0);
        let view_proj = directx::perspective(0.5, 1.0, 1000.0, 0.1) * Mat4::from_translation(-eye);
        let view = CullView::camera(view_proj, eye, true, Culling::default());
        let models = [
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(100.0, 0.0, 0.0)),
            Mat4::from_translation(Vec3::new(-6.0, -3.0, 0.0)),
        ];
        let draws = [path(0, key(1)), path(1, key(1)), path(2, key(2))];
        let plan = plan(&draws, 0);
        let mut up = Uploads::new();
        let mut keep = Vec::new();
        let tables = tables(device, &mut up, (&models, &[view]), &mut keep);
        let pipelines = RoutePipelines::ComputeIndirect {
            cull: cull_pipelines(device),
            shadow: Pipeline::from_raw(0, 0),
        };
        let job = MeshletJob::prepare(device, &mut up, &plan, (tables, pipelines), &mut keep)
            .unwrap()
            .unwrap();
        up.flush(device).unwrap();
        let compute = job.compute.expect("the compute route culls");
        run_commands(device, compute.commands());
        let codes = download(device, job.codes, u64::from(plan.entries) * 4);
        let entries = download(device, compute.entries, u64::from(plan.entries) * 8);
        let args = download(device, compute.args, plan.buckets.len() as u64 * 16);
        let drawn = |slot: usize| codes[slot] & 0xff == CullCode::Drawn as u32;
        assert!(
            (0..codes.len()).any(|s| !drawn(s)) && (0..codes.len()).any(drawn),
            "{codes:?}"
        );
        for (b, bucket) in plan.buckets.iter().enumerate() {
            let mut want = Vec::new();
            for item in plan.items.iter().filter(|i| i[3] == b as u32) {
                for m in 0..meshlets {
                    if drawn((item[2] + m) as usize) {
                        want.extend([item[0], m]);
                    }
                }
            }
            let start = bucket.first as usize * 2;
            assert_eq!(
                entries[start..start + want.len()],
                want[..],
                "bucket {b} in slot order"
            );
            assert_eq!(
                args[b * 4 + 1] as usize,
                want.len() / 2,
                "bucket {b}'s instance count"
            );
        }
    }

    #[test]
    fn cull_covers_every_entry_in_rows_of_groups() {
        assert_eq!(cull_groups(1), [1, 1, 1]);
        assert_eq!(cull_groups(64), [1, 1, 1]);
        assert_eq!(cull_groups(65), [2, 1, 1]);
        let row = 65_535 * 64;
        assert_eq!(cull_groups(row), [65_535, 1, 1]);
        assert_eq!(cull_groups(row + 1), [65_535, 2, 1]);
        assert_eq!(cull_groups(u32::MAX), [65_535, 1025, 1]);
        for entries in [1, 63, 64, 65, 1000, row - 1, row, row + 1, 3 * row + 7] {
            let [x, y, _] = cull_groups(entries);
            let threads = u64::from(x) * 64 * u64::from(y);
            assert!(threads >= u64::from(entries), "{entries}");
            assert!(y == 1 || x == ROW_GROUPS, "full rows first: {entries}");
        }
    }
}
