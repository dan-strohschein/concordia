//! Clustered lighting: a froxel grid in view space (exponential depth slices for perspective,
//! linear for orthographic) and the CPU reference light assignment the GPU pass must match.
//!
//! References: Olsson, Billeter & Assarsson, "Clustered Deferred and Forward Shading" (HPG
//! 2012); Emil Persson, "Practical Clustered Shading" (SIGGRAPH 2015).

use std::num::NonZeroU64;
use std::sync::Arc;

use concordia_core::math::Vec3;
use concordia_rhi::{
    Access, AccessFlags, Buffer, BufferBarrier, BufferDesc, BufferUsage, Command, CommandList,
    ComputePipelineDesc, Device, Layout, MemoryLocation, Pipeline, QueueKind, Stages, Submit,
};
use concordia_shaders::{ShaderCompiler, Stage};

use crate::frame::push_rows;
use crate::gpu::Uploads;
use crate::pipelines::compile;
use crate::{Projection, RenderError};

/// Grid size and light budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterSettings {
    /// Tiles across, tiles down, depth slices.
    pub dims: [u32; 3],
    /// Lights per frame.
    pub max_lights: u32,
    /// Lights per cluster (more are dropped and counted as overflow).
    pub max_per_cluster: u32,
}

impl Default for ClusterSettings {
    fn default() -> Self {
        Self {
            dims: [16, 9, 24],
            max_lights: 4096,
            max_per_cluster: 128,
        }
    }
}

/// A light's bounding sphere in view space (right-handed, looking down -Z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightBounds {
    /// Center.
    pub center: Vec3,
    /// Radius (the light's range).
    pub radius: f32,
}

/// Light indices per cluster, plus how many assignments did not fit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClusterLists {
    /// Ascending light indices per cluster.
    pub lists: Vec<Vec<u32>>,
    /// Assignments dropped because a cluster was full.
    pub overflow: u32,
}

/// The froxel grid of one camera.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterGrid {
    settings: ClusterSettings,
    near: f32,
    far: f32,
    ortho: bool,
    /// View-space half extents at depth 1 (perspective) or absolute (orthographic).
    half: (f32, f32),
}

impl ClusterGrid {
    /// The grid for a projection at `aspect`.
    pub fn new(
        settings: ClusterSettings,
        projection: &Projection,
        aspect: f32,
    ) -> Result<Self, RenderError> {
        projection.matrix(aspect)?;
        let [x, y, z] = settings.dims;
        if x == 0 || y == 0 || z == 0 || x * y * z > 1 << 16 {
            return Err(RenderError::InvalidCamera(format!(
                "cluster dims {:?} must be non-zero and ≤ 65536 clusters",
                settings.dims
            )));
        }
        if settings.max_per_cluster == 0 || settings.max_lights == 0 {
            return Err(RenderError::InvalidCamera(
                "cluster light budgets must be non-zero".into(),
            ));
        }
        let (near, far, ortho, half) = match *projection {
            Projection::Perspective { fov_y, near, far } => {
                let t = (fov_y / 2.0).tan();
                (near, far, false, (t * aspect, t))
            }
            Projection::Orthographic { height, near, far } => {
                (near, far, true, (height * aspect / 2.0, height / 2.0))
            }
        };
        Ok(Self {
            settings,
            near,
            far,
            ortho,
            half,
        })
    }

    /// The settings.
    pub fn settings(&self) -> &ClusterSettings {
        &self.settings
    }

    /// Number of clusters.
    pub fn len(&self) -> usize {
        self.settings.dims.iter().product::<u32>() as usize
    }

    /// True if the grid has no clusters (never, by construction).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Near and far view distances, and whether slices are linear (orthographic).
    pub fn depth_range(&self) -> (f32, f32, bool) {
        (self.near, self.far, self.ortho)
    }

    /// The `dims[2] + 1` view distances bounding the depth slices.
    pub fn slice_boundaries(&self) -> Vec<f32> {
        let n = self.settings.dims[2];
        (0..=n)
            .map(|k| self.slice_depth(k as f32 / n as f32))
            .collect()
    }

    fn slice_depth(&self, t: f32) -> f32 {
        match self.ortho {
            true => self.near + (self.far - self.near) * t,
            false => self.near * (self.far / self.near).powf(t),
        }
    }

    /// View-space AABB (min, max) of cluster `index` (x fastest, tile rows from the top, then slices).
    pub fn aabb(&self, index: usize) -> (Vec3, Vec3) {
        let [dx, dy, dz] = self.settings.dims.map(|d| d as usize);
        let (tx, ty, tz) = (index % dx, (index / dx) % dy, index / (dx * dy));
        let ndc_x = |t: usize| -1.0 + 2.0 * t as f32 / dx as f32;
        let ndc_y = |t: usize| 1.0 - 2.0 * t as f32 / dy as f32;
        let (z0, z1) = (
            self.slice_depth(tz as f32 / dz as f32),
            self.slice_depth((tz + 1) as f32 / dz as f32),
        );
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for (nx, ny, z) in [
            (ndc_x(tx), ndc_y(ty)),
            (ndc_x(tx + 1), ndc_y(ty + 1)),
            (ndc_x(tx), ndc_y(ty + 1)),
            (ndc_x(tx + 1), ndc_y(ty)),
        ]
        .into_iter()
        .flat_map(|(x, y)| [(x, y, z0), (x, y, z1)])
        {
            let scale = if self.ortho { 1.0 } else { z };
            let p = Vec3::new(nx * self.half.0 * scale, ny * self.half.1 * scale, -z);
            min = min.min(p);
            max = max.max(p);
        }
        (min, max)
    }

    /// The CPU reference: each cluster lists the lights whose sphere touches its AABB.
    pub fn assign(&self, lights: &[LightBounds]) -> ClusterLists {
        let cap = self.settings.max_per_cluster as usize;
        let mut out = ClusterLists {
            lists: vec![Vec::new(); self.len()],
            overflow: 0,
        };
        for (c, list) in out.lists.iter_mut().enumerate() {
            let (min, max) = self.aabb(c);
            for (i, light) in lights.iter().enumerate() {
                if sphere_touches_aabb(light, min, max) {
                    match list.len() < cap {
                        true => list.push(i as u32),
                        false => out.overflow += 1,
                    }
                }
            }
        }
        out
    }
}

/// Squared distance from the sphere center to the box, against the squared radius.
pub(crate) fn sphere_touches_aabb(light: &LightBounds, min: Vec3, max: Vec3) -> bool {
    let d = light.center.clamp(min, max) - light.center;
    d.dot(d) <= light.radius * light.radius
}

/// The GPU light-culling pass (one compute dispatch; results equal [`ClusterGrid::assign`]).
pub struct ClusterCuller {
    device: Arc<dyn Device>,
    pipeline: Pipeline,
}

/// One frame's clustering buffers (bindless indices for the shaders).
pub(crate) struct ClusterBuffers {
    pub(crate) counts: Buffer,
    pub(crate) indices: Buffer,
    pub(crate) counts_index: u32,
    pub(crate) indices_index: u32,
    aabbs_index: u32,
    bounds_index: u32,
    owned: Vec<Buffer>,
}

impl ClusterBuffers {
    /// Every buffer, for the caller to free after the frame.
    pub(crate) fn buffers(&self) -> &[Buffer] {
        &self.owned
    }
}

const WRITE: Access = Access {
    stages: Stages::COMPUTE,
    access: AccessFlags::SHADER_WRITE,
    layout: Layout::General,
};

impl ClusterCuller {
    /// Compiles the culling shader.
    pub fn new(
        device: Arc<dyn Device>,
        compiler: &dyn ShaderCompiler,
    ) -> Result<Self, RenderError> {
        let module = compile(
            device.as_ref(),
            compiler,
            "cluster.slang",
            "cull",
            Stage::Compute,
        )?;
        let threads_per_group = module.reflection.threads_per_group.unwrap_or([64, 1, 1]);
        let pipeline = device.create_compute_pipeline(&ComputePipelineDesc {
            shader: module.source(),
            threads_per_group,
            label: "cluster cull".into(),
        })?;
        Ok(Self { device, pipeline })
    }

    /// Uploads cluster AABBs and light bounds and creates the output buffers.
    pub(crate) fn prepare(
        &self,
        up: &mut Uploads,
        grid: &ClusterGrid,
        lights: &[LightBounds],
    ) -> Result<ClusterBuffers, RenderError> {
        let device = self.device.as_ref();
        let mut boxes = Vec::with_capacity(grid.len() * 32);
        for c in 0..grid.len() {
            let (min, max) = grid.aabb(c);
            push_rows(&mut boxes, &[min.extend(0.0), max.extend(0.0)]);
        }
        let mut bounds = Vec::with_capacity(lights.len() * 16);
        lights
            .iter()
            .for_each(|l| push_rows(&mut bounds, &[l.center.extend(l.radius)]));
        let aabbs = up.buffer(device, &boxes, "cluster aabbs")?;
        let bounds = up.buffer(device, &bounds, "light bounds")?;
        let output = |bytes: usize, label: &str| {
            up.alloc(
                device,
                &BufferDesc {
                    size: NonZeroU64::new(bytes as u64).unwrap_or(NonZeroU64::MIN),
                    usage: BufferUsage::STORAGE | BufferUsage::BINDLESS | BufferUsage::COPY_SRC,
                    memory: MemoryLocation::GpuOnly,
                    label: label.into(),
                },
            )
        };
        let counts = output(grid.len() * 8, "cluster counts")?;
        let indices = output(
            grid.len() * grid.settings.max_per_cluster as usize * 4,
            "cluster indices",
        )?;
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        Ok(ClusterBuffers {
            counts,
            indices,
            counts_index: index(counts)?,
            indices_index: index(indices)?,
            aabbs_index: index(aabbs)?,
            bounds_index: index(bounds)?,
            owned: vec![aabbs, bounds, counts, indices],
        })
    }

    /// Records the dispatch and a barrier making the lists visible to `after`.
    pub(crate) fn record(
        &self,
        list: &mut CommandList,
        b: &ClusterBuffers,
        grid: &ClusterGrid,
        light_count: usize,
        after: Access,
    ) {
        let clusters = grid.len() as u32;
        let words = [
            b.aabbs_index,
            b.bounds_index,
            light_count as u32,
            b.counts_index,
            b.indices_index,
            clusters,
            grid.settings.max_per_cluster,
            0,
        ];
        list.push(Command::SetPipeline(self.pipeline))
            .push(Command::PushConstants(
                words.iter().flat_map(|w| w.to_le_bytes()).collect(),
            ))
            .push(Command::Dispatch {
                groups: [clusters.div_ceil(64), 1, 1],
            })
            .push(Command::Barrier {
                buffers: [b.counts, b.indices]
                    .map(|buffer| BufferBarrier {
                        buffer,
                        before: WRITE,
                        after,
                    })
                    .to_vec(),
                textures: Vec::new(),
            });
    }

    /// Culls on the GPU and reads the lists back (tests and tools).
    pub fn cull(
        &self,
        grid: &ClusterGrid,
        lights: &[LightBounds],
    ) -> Result<ClusterLists, RenderError> {
        let device = self.device.as_ref();
        let mut up = Uploads::new();
        let buffers = self.prepare(&mut up, grid, lights)?;
        up.flush(device)?;
        let result = self.cull_and_read(grid, lights.len(), &buffers);
        buffers
            .buffers()
            .iter()
            .for_each(|b| device.destroy_buffer(*b));
        result
    }

    fn cull_and_read(
        &self,
        grid: &ClusterGrid,
        light_count: usize,
        b: &ClusterBuffers,
    ) -> Result<ClusterLists, RenderError> {
        let device = self.device.as_ref();
        let copy = Access {
            stages: Stages::COPY,
            access: AccessFlags::COPY_READ,
            layout: Layout::General,
        };
        let (count_bytes, index_bytes) = (
            grid.len() as u64 * 8,
            grid.len() as u64 * u64::from(grid.settings.max_per_cluster) * 4,
        );
        let readback = |size: u64| {
            device.create_buffer(&BufferDesc {
                size: NonZeroU64::new(size).unwrap_or(NonZeroU64::MIN),
                usage: BufferUsage::COPY_DST,
                memory: MemoryLocation::Readback,
                label: "cluster readback".into(),
            })
        };
        let (counts_rb, indices_rb) = (readback(count_bytes)?, readback(index_bytes)?);
        let mut list = CommandList::new("cluster cull");
        self.record(&mut list, b, grid, light_count, copy);
        list.push(Command::CopyBuffer {
            src: b.counts,
            src_offset: 0,
            dst: counts_rb,
            dst_offset: 0,
            size: count_bytes,
        })
        .push(Command::CopyBuffer {
            src: b.indices,
            src_offset: 0,
            dst: indices_rb,
            dst_offset: 0,
            size: index_bytes,
        });
        let submitted = device
            .submit(Submit {
                queue: QueueKind::Graphics,
                commands: &list,
                wait: &[],
                signal: &[],
            })
            .and_then(|()| device.wait_idle())
            .and_then(|()| {
                Ok((
                    device.read_buffer(counts_rb, 0, count_bytes)?,
                    device.read_buffer(indices_rb, 0, index_bytes)?,
                ))
            });
        device.destroy_buffer(counts_rb);
        device.destroy_buffer(indices_rb);
        let (counts, indices) = submitted?;
        Ok(decode_lists(grid, &words(&counts), &words(&indices)))
    }
}

fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect()
}

/// Turns the GPU's counts (first half), dropped counts (second half) and slots into lists.
pub(crate) fn decode_lists(grid: &ClusterGrid, counts: &[u32], indices: &[u32]) -> ClusterLists {
    let (n, cap) = (grid.len(), grid.settings.max_per_cluster as usize);
    ClusterLists {
        lists: (0..n)
            .map(|c| indices[c * cap..c * cap + counts[c] as usize].to_vec())
            .collect(),
        overflow: counts[n..2 * n].iter().sum(),
    }
}
