//! GPU side of the meshlet path (ADR-0100): the frame's meshlet buffers and each bucket's draw,
//! by the compute + indirect route (the `meshlet.expand` pass, M4 4.3a) or the mesh-shader route
//! (one mesh-task draw per bucket, 4.3b).

use std::num::NonZeroU64;

use concordia_rhi::{
    Access, AccessFlags, Buffer, BufferBarrier, BufferDesc, BufferUsage, Command, Device, Layout,
    MemoryLocation, Pipeline, Stages,
};

use crate::RenderError;
use crate::frame::push_rows_u32;
use crate::gpu::Uploads;
use crate::graph::{PassContext, RenderGraph};
use crate::meshlet::{ExpandPush, MeshletPlan, MeshletPush, Route, TaskPush, task_groups};

/// A forward bucket's key: its pipeline and its material's first row.
pub(crate) type BucketKey = (Pipeline, u32);

/// Groups of 64 threads per dispatch row (the most a dimension allows everywhere).
const ROW_GROUPS: u32 = 65_535;

/// The dispatch size covering `entries` threads (64 per group, rows of [`ROW_GROUPS`] groups).
pub(crate) fn expand_groups(entries: u32) -> [u32; 3] {
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
}

/// The pipelines a route draws with: forward pipelines come with the buckets' keys.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RoutePipelines {
    /// `meshlet.expand` and the vertex-shader shadow pipeline.
    ComputeIndirect { expand: Pipeline, shadow: Pipeline },
    /// The mesh-shader shadow pipeline.
    MeshTasks { shadow: Pipeline },
}

/// One bucket's draw, ready to record: its pipeline, push words (the shadow cascade last, in both
/// routes' layouts) and draw command.
#[derive(Debug, Clone)]
pub(crate) struct BucketDraw {
    pub(crate) pipeline: Pipeline,
    push: Vec<u32>,
    draw: Command,
}

impl BucketDraw {
    /// The push constants and draw (the pipeline is set by the caller), for `cascade` if a shadow.
    pub(crate) fn commands(&self, cascade: Option<u32>) -> [Command; 2] {
        let mut push = self.push.clone();
        if let (Some(c), Some(last)) = (cascade, push.last_mut()) {
            *last = c;
        }
        let bytes = push.iter().flat_map(|w| w.to_le_bytes()).collect();
        [Command::PushConstants(bytes), self.draw.clone()]
    }
}

/// `meshlet.expand`: its pipeline, push constants and dispatch, and the buffers it writes.
#[derive(Debug, Clone)]
struct Expand {
    pipeline: Pipeline,
    push: ExpandPush,
    groups: [u32; 3],
    entries: Buffer,
    /// One `DrawIndirect` record per bucket.
    args: Buffer,
}

/// The frame's meshlet work.
#[derive(Debug, Clone)]
pub(crate) struct MeshletJob {
    /// How the buckets reach the rasterizer.
    pub(crate) route: Route,
    /// The compute route's expand pass.
    expand: Option<Expand>,
    /// Forward buckets, in plan order.
    pub(crate) forward: Vec<BucketDraw>,
    /// The shadow casters' bucket (cascade filled per pass).
    pub(crate) shadow: Option<BucketDraw>,
}

impl MeshletJob {
    /// Uploads `plan`'s work items (and, on the compute route, its bucket table, entries and
    /// arguments); `None` when nothing takes the path. Every buffer is returned in `keep`.
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
        let mut items = Vec::new();
        push_rows_u32(&mut items, &plan.items);
        let items = up.buffer(device, &items, "meshlet work items")?;
        keep.push(items);
        let items = device.buffer_index(items)?.0;
        let (route, expand, shadow_pipeline) = match pipelines {
            RoutePipelines::ComputeIndirect { expand, shadow } => {
                let job = Expand::prepare(device, up, plan, (items, expand), keep)?;
                (Route::ComputeIndirect, Some(job), shadow)
            }
            RoutePipelines::MeshTasks { shadow } => (Route::MeshTasks, None, shadow),
        };
        let draw = |pipeline, material, i: usize| {
            let bucket = &plan.buckets[i];
            match &expand {
                Some(e) => BucketDraw {
                    pipeline,
                    push: MeshletPush {
                        frame: tables.frame,
                        instances: tables.instances,
                        entries: e.push.entries,
                        materials: tables.materials,
                        material,
                        meshes: tables.meshes,
                        first: bucket.first,
                        cascade: 0,
                    }
                    .words()
                    .to_vec(),
                    draw: Command::DrawIndirect {
                        args: e.args,
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
                        cascade: 0,
                    }
                    .words()
                    .to_vec(),
                    draw: Command::DrawMeshTasks {
                        groups: task_groups(bucket.entries),
                    },
                },
            }
        };
        let mut forward = Vec::new();
        let mut shadow = None;
        for (i, bucket) in plan.buckets.iter().enumerate() {
            match bucket.key {
                Some((pipeline, material)) => forward.push(draw(pipeline, material, i)),
                None => shadow = Some(draw(shadow_pipeline, 0, i)),
            }
        }
        Ok(Some(Self {
            route,
            expand,
            forward,
            shadow,
        }))
    }

    /// The `meshlet.expand` pass (compute route only).
    pub(crate) fn add_pass(&self, graph: &mut RenderGraph) {
        let Some(expand) = self.expand.clone() else {
            return;
        };
        graph.add_pass_recorded(
            "meshlet.expand",
            |p| p.side_effect(),
            Some(Box::new(move |ctx: &mut PassContext<'_>| {
                ctx.commands.commands.extend(expand.commands());
                Ok(())
            })),
        );
    }
}

impl Expand {
    /// Uploads the bucket table and allocates the entries and arguments `meshlet.expand` writes;
    /// `items` is the work items' bindless index.
    fn prepare(
        device: &dyn Device,
        up: &mut Uploads,
        plan: &MeshletPlan<BucketKey>,
        (items, pipeline): (u32, Pipeline),
        keep: &mut Vec<Buffer>,
    ) -> Result<Self, RenderError> {
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        let mut buckets = Vec::new();
        let rows: Vec<[u32; 4]> = plan
            .buckets
            .iter()
            .map(|b| [b.first, b.entries, b.vertices, 0])
            .collect();
        push_rows_u32(&mut buckets, &rows);
        let bucket_table = up.buffer(device, &buckets, "meshlet buckets")?;
        let gpu = |bytes: u64, usage: BufferUsage, label: &str| {
            up.alloc(
                device,
                &BufferDesc {
                    size: NonZeroU64::new(bytes).unwrap_or(NonZeroU64::MIN),
                    usage: usage
                        | BufferUsage::STORAGE
                        | BufferUsage::BINDLESS
                        | BufferUsage::COPY_SRC,
                    memory: MemoryLocation::GpuOnly,
                    label: label.into(),
                },
            )
        };
        let entries = gpu(
            u64::from(plan.entries) * 8,
            BufferUsage::empty(),
            "meshlet entries",
        )?;
        let args = gpu(
            plan.buckets.len() as u64 * concordia_rhi::DRAW_INDIRECT_BYTES,
            BufferUsage::INDIRECT,
            "meshlet arguments",
        )?;
        keep.extend([bucket_table, entries, args]);
        Ok(Self {
            pipeline,
            push: ExpandPush {
                items,
                item_count: plan.items.len() as u32,
                buckets: index(bucket_table)?,
                entries: index(entries)?,
                args: index(args)?,
                total: plan.entries,
            },
            groups: expand_groups(plan.entries),
            entries,
            args,
        })
    }

    /// The pass's commands: the dispatch and its barrier.
    fn commands(&self) -> [Command; 4] {
        [
            Command::SetPipeline(self.pipeline),
            Command::PushConstants(self.push.bytes()),
            Command::Dispatch {
                groups: self.groups,
            },
            self.barrier(),
        ]
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
    use crate::meshlet::{MeshletInfo, PathDraw, plan};
    use concordia_rhi::{CommandList, QueueKind, Submit};

    fn device() -> Option<std::sync::Arc<dyn Device>> {
        let backend = concordia_rhi_select::choose_from_env(Default::default()).ok()?;
        match concordia_rhi_select::new_headless(backend) {
            Ok(d) => Some(d.into()),
            Err(e) => {
                assert!(!concordia_testkit::device_required(backend), "{e}");
                None
            }
        }
    }

    fn download(device: &dyn Device, buffer: Buffer, bytes: u64) -> Vec<u32> {
        let readback = device
            .create_buffer(&BufferDesc {
                size: NonZeroU64::new(bytes).unwrap(),
                usage: BufferUsage::COPY_DST,
                memory: MemoryLocation::Readback,
                label: "readback".into(),
            })
            .unwrap();
        let mut list = CommandList::new("download");
        list.push(Command::CopyBuffer {
            src: buffer,
            src_offset: 0,
            dst: readback,
            dst_offset: 0,
            size: bytes,
        });
        run(device, &list);
        let raw = device.read_buffer(readback, 0, bytes).unwrap();
        raw.as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect()
    }

    fn run(device: &dyn Device, list: &CommandList) {
        device
            .submit(Submit {
                queue: QueueKind::Graphics,
                commands: list,
                wait: &[],
                signal: &[],
            })
            .unwrap();
        device.wait_idle().unwrap();
    }

    #[test]
    fn expand_writes_one_entry_per_meshlet_and_counts_them() {
        let Some(device) = device() else { return };
        let device = device.as_ref();
        let toolchain =
            concordia_shaders::SlangToolchain::locate().expect("run `just slang-fetch`");
        let compiler = concordia_shaders::SlangcCompiler::new(toolchain);
        let module = crate::pipelines::compile(
            device,
            &compiler,
            "meshlet.slang",
            "meshlet_expand",
            concordia_shaders::Stage::Compute,
        )
        .unwrap();
        let expand = device
            .create_compute_pipeline(&concordia_rhi::ComputePipelineDesc {
                shader: module.source(),
                threads_per_group: module.reflection.threads_per_group.unwrap(),
                label: "expand".into(),
            })
            .unwrap();
        let key = |m| Some((Pipeline::from_raw(0, 0), m));
        let path = |instance, meshlets, forward, casts| PathDraw {
            instance,
            mesh_row: instance % 2,
            info: MeshletInfo {
                meshlets,
                max_triangles: 128,
            },
            forward,
            casts,
        };
        // Forward buckets of 3 + 1, 70 and 1 entries (one-entry buckets catch an off-by-one in
        // who writes the arguments), then 70 + 1 + 5 shadow entries.
        let draws = [
            path(4, 3, key(9), false),
            path(5, 70, key(2), true),
            path(6, 1, key(9), true),
            path(8, 1, key(3), false),
            path(7, 5, None, true),
        ];
        let plan = plan(&draws);
        let mut up = Uploads::new();
        let mut keep = Vec::new();
        let tables = FrameTables {
            frame: 0,
            instances: 0,
            materials: 0,
            meshes: 0,
        };
        let pipelines = RoutePipelines::ComputeIndirect {
            expand,
            shadow: expand,
        };
        let job = MeshletJob::prepare(device, &mut up, &plan, (tables, pipelines), &mut keep)
            .unwrap()
            .unwrap();
        up.flush(device).unwrap();
        let job = job.expand.expect("the compute route expands");
        let mut list = CommandList::new("expand");
        job.commands().into_iter().for_each(|c| {
            list.push(c);
        });
        run(device, &list);
        let entries = download(device, job.entries, u64::from(plan.entries) * 8);
        let mut want = Vec::new();
        for item in &plan.items {
            let meshlets = draws
                .iter()
                .find(|d| d.instance == item[0])
                .unwrap()
                .info
                .meshlets;
            for m in 0..meshlets {
                want.extend([item[0], m]);
            }
        }
        assert_eq!(
            entries, want,
            "one (instance, meshlet) per entry, in item order"
        );
        let args = download(device, job.args, plan.buckets.len() as u64 * 16);
        let want_args: Vec<u32> = plan
            .buckets
            .iter()
            .flat_map(|b| [b.vertices, b.entries, 0, 0])
            .collect();
        assert_eq!(args, want_args, "each bucket's draw arguments");
        assert_eq!(plan.buckets.len(), 4);
    }

    #[test]
    fn expand_covers_every_entry_in_rows_of_groups() {
        assert_eq!(expand_groups(1), [1, 1, 1]);
        assert_eq!(expand_groups(64), [1, 1, 1]);
        assert_eq!(expand_groups(65), [2, 1, 1]);
        let row = 65_535 * 64;
        assert_eq!(expand_groups(row), [65_535, 1, 1]);
        assert_eq!(expand_groups(row + 1), [65_535, 2, 1]);
        assert_eq!(expand_groups(u32::MAX), [65_535, 1025, 1]);
        for entries in [1, 63, 64, 65, 1000, row - 1, row, row + 1, 3 * row + 7] {
            let [x, y, _] = expand_groups(entries);
            let threads = u64::from(x) * 64 * u64::from(y);
            assert!(threads >= u64::from(entries), "{entries}");
            assert!(y == 1 || x == ROW_GROUPS, "full rows first: {entries}");
        }
    }
}
