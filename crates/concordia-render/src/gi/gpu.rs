//! The probe update on the GPU: trace every probe's rays through GI's scene, shade the hits,
//! blend irradiance and distance into the atlases, and relocate probes (`shaders/gi_update.slang`).

use std::num::NonZeroU64;

use concordia_bvh::GpuBvh;
use concordia_core::math::{IVec3, Mat3, UVec3, Vec3};
use concordia_rhi::{
    AccelDesc, AccelInstance, AccelerationStructure, Buffer, BufferDesc, BufferUsage, Command,
    CommandList, Device, MemoryLocation, Pipeline, QueueKind, Submit, TriangleGeometry,
};
use concordia_shaders::ShaderCompiler;

use super::GiScene;
use crate::RenderError;
use crate::pipelines::{compute_pipeline, compute_pipeline_with};
use crate::scan::compute_barrier;

/// The probe lattice: probe (i, j, k) sits at `origin + (i, j, k) · spacing` (plus its offset).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProbeGrid {
    /// Probe (0, 0, 0)'s grid point.
    pub origin: Vec3,
    /// Probes along each axis.
    pub counts: UVec3,
    /// Metres between neighbours.
    pub spacing: f32,
}

impl Default for ProbeGrid {
    fn default() -> Self {
        Self {
            origin: Vec3::ZERO,
            counts: UVec3::new(32, 8, 32),
            spacing: 1.5,
        }
    }
}

/// The sun as GI sees it (physical units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GiSun {
    /// The direction light travels.
    pub direction: Vec3,
    /// Illuminance (lux) × colour, on a surface facing the sun.
    pub illuminance: Vec3,
}

/// A point or spot light as GI sees it (physical units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GiLight {
    /// Position.
    pub position: Vec3,
    /// Reach in metres.
    pub range: f32,
    /// The spot's axis (unused for points).
    pub direction: Vec3,
    /// Intensity (candela) × colour.
    pub intensity: Vec3,
    /// Spot attenuation `saturate(cos · scale + offset)²` (points: 0, 1).
    pub spot: (f32, f32),
    /// Casts shadows (GI traces a shadow ray to it).
    pub shadowed: bool,
}

/// One frame's inputs to the probe update.
#[derive(Debug, Clone)]
pub struct GiInputs {
    /// The lattice this frame.
    pub grid: ProbeGrid,
    /// Rays per probe.
    pub rays: u32,
    /// The frame number (rotates the rays).
    pub frame: u32,
    /// The sun, if any.
    pub sun: Option<GiSun>,
    /// Point and spot lights.
    pub lights: Vec<GiLight>,
    /// The sky's irradiance as SH9 (physical), if any: what rays that miss see.
    pub sky: Option<[Vec3; 9]>,
    /// Where lights appeared this frame (probes near them react at once).
    pub light_events: Vec<Vec3>,
    /// The old value's weight when nothing changes (a warm-up lowers it to converge fast).
    pub hysteresis: f32,
    /// Move probes out of geometry this update (the renderer does it while a lattice settles;
    /// relocating every frame on rotating rays jitters probes near the threshold).
    pub relocate: bool,
}

impl Default for GiInputs {
    fn default() -> Self {
        Self {
            grid: ProbeGrid::default(),
            rays: 256,
            frame: 0,
            sun: None,
            lights: Vec::new(),
            sky: None,
            light_events: Vec::new(),
            hysteresis: super::BASE_HYSTERESIS,
            relocate: true,
        }
    }
}

/// Rows of the parameter block (`gi.slang`).
const PARAM_ROWS: usize = 24;
/// Bordered tile sizes in texels (`gi.slang`).
const IRR_TILE: u64 = 10;
const DIST_TILE: u64 = 18;
/// The sampling biases along the normal and toward the eye, in probe spacings.
const NORMAL_BIAS: f32 = 0.2;
const VIEW_BIAS: f32 = 0.1;

/// The update's kernels.
#[derive(Debug, Clone, Copy)]
struct Kernels {
    trace: Pipeline,
    /// The trace on hardware ray queries (devices with ray tracing, M4 4.8c).
    trace_rq: Option<Pipeline>,
    blend_irradiance: Pipeline,
    blend_distance: Pipeline,
    border: Pipeline,
    relocate: Pipeline,
    scroll: Pipeline,
}

/// A bindless buffer GI writes from the CPU or the GPU (Upload memory, grown as needed).
#[derive(Debug, Default)]
struct Slot {
    buffer: Option<Buffer>,
    size: u64,
    index: u32,
}

impl Slot {
    /// Holds at least `size` bytes (contents undefined after growing).
    fn ensure(&mut self, device: &dyn Device, size: u64, label: &str) -> Result<(), RenderError> {
        let size = size.max(16);
        if self.buffer.is_some() && self.size >= size {
            return Ok(());
        }
        if let Some(b) = self.buffer.take() {
            device.destroy_buffer(b);
        }
        let buffer = device.create_buffer(&BufferDesc {
            size: NonZeroU64::new(size).unwrap_or(NonZeroU64::MIN),
            usage: BufferUsage::STORAGE | BufferUsage::BINDLESS | BufferUsage::COPY_SRC,
            memory: MemoryLocation::Upload,
            label: label.into(),
        })?;
        self.index = device.buffer_index(buffer)?.0;
        self.buffer = Some(buffer);
        self.size = size;
        Ok(())
    }

    /// Writes `bytes` (growing first).
    fn write(&mut self, device: &dyn Device, bytes: &[u8], label: &str) -> Result<(), RenderError> {
        self.ensure(device, bytes.len() as u64, label)?;
        if let Some(b) = self.buffer
            && !bytes.is_empty()
        {
            device.write_buffer(b, 0, bytes)?;
        }
        Ok(())
    }

    fn buffer(&self) -> Result<Buffer, RenderError> {
        self.buffer
            .ok_or_else(|| RenderError::Shader("a GI buffer was used before it existed".into()))
    }
}

/// The probes' GPU state and kernels.
pub struct GiProbes {
    kernels: Kernels,
    params: Slot,
    probes: Slot,
    rays: Slot,
    irradiance: Slot,
    distance: Slot,
    bvh: [Slot; 5],
    looks: Slot,
    instance_looks: Slot,
    lights: Slot,
    events: Slot,
    /// The instance tree's first node and order entry.
    tlas: (u32, u32),
    /// The scene as acceleration structures (with ray queries).
    hardware: Option<HardwareScene>,
    /// The lattice the atlases hold (a different one resets them).
    grid: Option<ProbeGrid>,
}

impl GiProbes {
    /// Compiles the kernels.
    pub fn new(device: &dyn Device, compiler: &dyn ShaderCompiler) -> Result<Self, RenderError> {
        let kernel = |entry: &str| compute_pipeline(device, compiler, "gi_update.slang", entry);
        let trace_rq = match device.caps().ray_tracing {
            true => Some(compute_pipeline_with(
                device,
                compiler,
                "gi_update.slang",
                "gi_trace",
                [("GI_RAY_QUERY".to_owned(), "1".to_owned())].into(),
            )?),
            false => None,
        };
        Ok(Self {
            hardware: trace_rq.map(|_| HardwareScene::default()),
            kernels: Kernels {
                trace_rq,
                trace: kernel("gi_trace")?,
                blend_irradiance: kernel("gi_blend_irradiance")?,
                blend_distance: kernel("gi_blend_distance")?,
                border: kernel("gi_border")?,
                relocate: kernel("gi_relocate")?,
                scroll: kernel("gi_scroll")?,
            },
            params: Slot::default(),
            probes: Slot::default(),
            rays: Slot::default(),
            irradiance: Slot::default(),
            distance: Slot::default(),
            bvh: Default::default(),
            looks: Slot::default(),
            instance_looks: Slot::default(),
            lights: Slot::default(),
            events: Slot::default(),
            tlas: (u32::MAX, 0),
            grid: None,
        })
    }

    /// Uploads this frame's scene and inputs and returns the update's commands (record them in
    /// order; the atlases are ready for sampling after them).
    pub fn prepare(
        &mut self,
        device: &dyn Device,
        scene: &GiScene,
        inputs: &GiInputs,
    ) -> Result<GiPasses, RenderError> {
        let grid = valid(inputs)?;
        let probes = u64::from(grid.counts.x * grid.counts.y * grid.counts.z);
        let rays = u64::from(inputs.rays);
        let reset = self.resets_for(&grid);
        // A move by whole cells scrolls: probes still inside keep their slots (M4 4.8d).
        let scrolled = match self.grid {
            Some(old) if !reset && old.origin != grid.origin => Some(origin_cell(&old)),
            _ => None,
        };
        if reset {
            let zeros = |n: u64| vec![0u8; n as usize];
            self.probes
                .write(device, &zeros(probes * 16), "gi probes")?;
            self.irradiance.write(
                device,
                &zeros(probes * IRR_TILE * IRR_TILE * 16),
                "gi irradiance",
            )?;
            self.distance.write(
                device,
                &zeros(probes * DIST_TILE * DIST_TILE * 8),
                "gi distance",
            )?;
            self.grid = Some(grid);
        }
        self.rays.ensure(device, probes * rays * 16, "gi rays")?;
        self.upload_scene(device, scene)?;
        let tlas = match self.hardware.as_mut() {
            Some(hw) => Some(hw.sync(device, scene)?),
            None => None,
        };
        self.lights
            .write(device, &bytes(&light_rows(&inputs.lights)), "gi lights")?;
        let events: Vec<[f32; 4]> = inputs
            .light_events
            .iter()
            .map(|e| e.extend(0.0).to_array())
            .collect();
        self.events.write(device, &bytes(&events), "gi events")?;
        self.grid = Some(grid);
        let mut rows = self.param_rows(inputs, reset);
        rows[22] = [tlas.unwrap_or(0), u32::from(tlas.is_some()), 0, 0];
        if let Some(old) = scrolled {
            rows[23] = [old[0] as u32, old[1] as u32, old[2] as u32, 1];
        }
        self.params.write(device, &words(&rows), "gi params")?;
        self.commands(probes, rays, scrolled.is_some())
    }

    /// Runs one update (uploads, traces, blends; waits for the GPU).
    pub fn update(
        &mut self,
        device: &dyn Device,
        scene: &GiScene,
        inputs: &GiInputs,
    ) -> Result<(), RenderError> {
        let passes = self.prepare(device, scene, inputs)?;
        let mut list = CommandList::new("gi update");
        for c in passes.into_iter().flat_map(|(_, commands)| commands) {
            list.push(c);
        }
        submit(device, &list)
    }

    /// Whether the update traces with hardware ray queries (else the software BVH).
    pub fn ray_queries(&self) -> bool {
        self.hardware.is_some()
    }

    /// Whether the next update with `grid` starts over: the first one, or different counts or
    /// spacing, or an origin moved by less than whole cells (a move by whole cells scrolls).
    pub fn resets_for(&self, grid: &ProbeGrid) -> bool {
        let Some(old) = self.grid else {
            return true;
        };
        if old.counts != grid.counts || old.spacing != grid.spacing {
            return true;
        }
        let cells = (grid.origin - old.origin) / grid.spacing;
        (cells - cells.round()).abs().max_element() > 1e-3
    }

    /// The slot of world cell `cell` in lattice `grid` (`None` outside it).
    pub fn probe_at(&self, grid: &ProbeGrid, cell: [i32; 3]) -> Option<u32> {
        let origin = origin_cell(grid);
        let counts = grid.counts.as_ivec3();
        let rel = IVec3::from(cell) - IVec3::from(origin);
        if rel.min_element() < 0 || (rel - counts).max_element() >= 0 {
            return None;
        }
        let slot = IVec3::from(cell).rem_euclid(counts).as_uvec3();
        Some(slot.x + grid.counts.x * (slot.y + grid.counts.y * slot.z))
    }

    /// The parameter block's bindless index (what sampling reads; valid after `prepare`).
    pub(crate) fn params_index(&self) -> u32 {
        self.params.index
    }

    /// Probe `probe`'s irradiance texels (8 × 8, rows first; physical lux).
    pub fn irradiance(&self, device: &dyn Device, probe: u32) -> Result<Vec<Vec3>, RenderError> {
        let tile = (IRR_TILE * IRR_TILE * 16) as usize;
        let all = download(device, self.irradiance.buffer()?, self.irradiance.size)?;
        let start = probe as usize * tile;
        let texels = all
            .get(start..start + tile)
            .ok_or_else(|| RenderError::Shader(format!("probe {probe} is outside the atlas")))?;
        let f =
            |i: usize| f32::from_le_bytes([texels[i], texels[i + 1], texels[i + 2], texels[i + 3]]);
        Ok((0..64)
            .map(|k| {
                let (x, y) = (k % 8 + 1, k / 8 + 1);
                let at = (y * IRR_TILE as usize + x) * 16;
                Vec3::new(f(at), f(at + 4), f(at + 8))
            })
            .collect())
    }

    /// The scene's hierarchy, looks and per-instance looks.
    fn upload_scene(&mut self, device: &dyn Device, scene: &GiScene) -> Result<(), RenderError> {
        let packed = GpuBvh::pack(scene.accel());
        let order: Vec<u8> = packed.order.iter().flat_map(|w| w.to_le_bytes()).collect();
        let parts: [(Vec<u8>, &str); 5] = [
            (words(&packed.nodes), "gi bvh nodes"),
            (order, "gi bvh order"),
            (words(&packed.tris), "gi bvh triangles"),
            (words(&packed.meshes), "gi bvh meshes"),
            (words(&packed.instances), "gi bvh instances"),
        ];
        for (slot, (data, label)) in self.bvh.iter_mut().zip(parts) {
            slot.write(device, &data, label)?;
        }
        self.tlas = (packed.tlas_node, packed.tlas_order);
        let looks: Vec<[f32; 4]> = scene
            .looks()
            .iter()
            .flat_map(|l| {
                [
                    l.albedo.extend(0.0).to_array(),
                    l.emission.extend(0.0).to_array(),
                ]
            })
            .collect();
        self.looks.write(device, &bytes(&looks), "gi looks")?;
        let per: Vec<u8> = scene
            .instance_materials()
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        self.instance_looks.write(device, &per, "gi instance looks")
    }

    /// The parameter block (`gi.slang`).
    fn param_rows(&self, inputs: &GiInputs, reset: bool) -> [[u32; 4]; PARAM_ROWS] {
        let g = inputs.grid;
        let f = |v: [f32; 4]| v.map(f32::to_bits);
        let mut rows = [[0u32; 4]; PARAM_ROWS];
        rows[0] = f([g.origin.x, g.origin.y, g.origin.z, g.spacing]);
        rows[1] = [g.counts.x, g.counts.y, g.counts.z, inputs.rays];
        rows[2] = [
            self.probes.index,
            self.rays.index,
            self.irradiance.index,
            self.distance.index,
        ];
        if let Some(sun) = inputs.sun {
            rows[3] = f(sun.direction.extend(0.0).to_array());
            rows[3][3] = 1;
            rows[4] = f(sun.illuminance.extend(0.0).to_array());
        }
        rows[4][3] = inputs.frame;
        rows[5] = [
            self.lights.index,
            inputs.lights.len() as u32,
            self.looks.index,
            self.instance_looks.index,
        ];
        rows[6] = [
            self.bvh[0].index,
            self.bvh[1].index,
            self.bvh[2].index,
            self.bvh[3].index,
        ];
        rows[7] = [
            self.bvh[4].index,
            self.tlas.0,
            self.tlas.1,
            u32::from(inputs.relocate),
        ];
        let rotation = ray_rotation(inputs.frame);
        for (k, r) in rows[8..11].iter_mut().enumerate() {
            *r = f(rotation.row(k).extend(0.0).to_array());
        }
        rows[11] = [
            u32::from(inputs.sky.is_some()),
            self.events.index,
            inputs.light_events.len() as u32,
            u32::from(reset),
        ];
        if let Some(sh) = inputs.sky {
            for (r, c) in rows[12..21].iter_mut().zip(sh) {
                *r = f(c.extend(0.0).to_array());
            }
        }
        rows[21] = f([
            inputs.hysteresis.clamp(0.0, 1.0),
            1.5 * g.spacing,
            NORMAL_BIAS,
            VIEW_BIAS,
        ]);
        rows
    }

    /// The update: trace, blend, borders, relocation, with barriers between.
    fn commands(&self, probes: u64, rays: u64, scrolled: bool) -> Result<GiPasses, RenderError> {
        let k = self.kernels;
        let push = |pass: u32| {
            Command::PushConstants(
                [self.params.index, pass, 0, 0]
                    .iter()
                    .flat_map(|w| w.to_le_bytes())
                    .collect(),
            )
        };
        let groups = |n: u64| [n.div_ceil(64) as u32, 1, 1];
        let (irr, dist, ray_buf, probe_buf) = (
            self.irradiance.buffer()?,
            self.distance.buffer()?,
            self.rays.buffer()?,
            self.probes.buffer()?,
        );
        let trace = match self.hardware.is_some() {
            true => k.trace_rq.unwrap_or(k.trace),
            false => k.trace,
        };
        let mut first = Vec::new();
        if scrolled {
            first.extend([
                Command::SetPipeline(k.scroll),
                push(0),
                Command::Dispatch {
                    groups: groups(probes),
                },
                compute_barrier(&[irr, dist, probe_buf]),
            ]);
        }
        let trace = [
            first,
            vec![
                Command::SetPipeline(trace),
                push(0),
                Command::Dispatch {
                    groups: groups(probes * rays),
                },
                compute_barrier(&[ray_buf]),
            ],
        ]
        .concat();
        let blend = vec![
            Command::SetPipeline(k.blend_irradiance),
            push(0),
            Command::Dispatch {
                groups: [probes as u32, 1, 1],
            },
            Command::SetPipeline(k.blend_distance),
            push(0),
            Command::Dispatch {
                groups: [probes as u32, 1, 1],
            },
            compute_barrier(&[irr, dist]),
        ];
        let finish = vec![
            Command::SetPipeline(k.border),
            push(0),
            Command::Dispatch {
                groups: groups(probes * IRR_TILE * IRR_TILE),
            },
            push(1),
            Command::Dispatch {
                groups: groups(probes * DIST_TILE * DIST_TILE),
            },
            Command::SetPipeline(k.relocate),
            push(0),
            Command::Dispatch {
                groups: groups(probes),
            },
            compute_barrier(&[irr, dist, probe_buf]),
        ];
        Ok(vec![
            ("gi.trace", trace),
            ("gi.blend", blend),
            ("gi.finish", finish),
        ])
    }
}

/// The update's stages, each a named list of commands (a graph pass apiece, so each is timed).
pub type GiPasses = Vec<(&'static str, Vec<Command>)>;

/// The world cell of the lattice's probe (0, 0, 0).
fn origin_cell(grid: &ProbeGrid) -> [i32; 3] {
    (grid.origin / grid.spacing).round().as_ivec3().to_array()
}

/// The grid, checked: counts 1..=256, a finite spacing and origin, 1..=512 rays (what the blend
/// kernels hold in group memory).
fn valid(inputs: &GiInputs) -> Result<ProbeGrid, RenderError> {
    let g = inputs.grid;
    let bad = |m: String| Err(RenderError::Shader(format!("GI: {m}")));
    if g.counts.min_element() == 0 || g.counts.max_element() > 256 {
        return bad(format!("probe counts {} must be 1..=256", g.counts));
    }
    if !(g.spacing.is_finite() && g.spacing > 0.0) || !g.origin.is_finite() {
        return bad(format!(
            "spacing {} and origin {} must be finite",
            g.spacing, g.origin
        ));
    }
    if !(1..=512).contains(&inputs.rays) {
        return bad(format!("{} rays per probe must be 1..=512", inputs.rays));
    }
    Ok(g)
}

/// Four rows per light: position + range, axis, intensity, (spot scale, offset, shadowed, 0).
fn light_rows(lights: &[GiLight]) -> Vec<[f32; 4]> {
    lights
        .iter()
        .flat_map(|l| {
            [
                l.position.extend(l.range).to_array(),
                l.direction.extend(0.0).to_array(),
                l.intensity.extend(0.0).to_array(),
                [l.spot.0, l.spot.1, if l.shadowed { 1.0 } else { 0.0 }, 0.0],
            ]
        })
        .collect()
}

/// A rotation that changes every frame (so the rays cover every direction over time).
fn ray_rotation(frame: u32) -> Mat3 {
    let hash = |x: u32| {
        let mut v = x.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        v = ((v >> ((v >> 28) + 4)) ^ v).wrapping_mul(277_803_737);
        ((v >> 22) ^ v) as f32 / u32::MAX as f32
    };
    let (a, b, c) = (
        hash(frame.wrapping_mul(3)),
        hash(frame.wrapping_mul(3) + 1),
        hash(frame.wrapping_mul(3) + 2),
    );
    let axis = Vec3::new(a - 0.5, b - 0.5, 0.5 - c).normalize_or(Vec3::Y);
    Mat3::from_axis_angle(axis, c * std::f32::consts::TAU)
}

fn bytes(rows: &[[f32; 4]]) -> Vec<u8> {
    rows.iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

fn words(rows: &[[u32; 4]]) -> Vec<u8> {
    rows.iter()
        .flatten()
        .flat_map(|w| w.to_le_bytes())
        .collect()
}

fn submit(device: &dyn Device, list: &CommandList) -> Result<(), RenderError> {
    device.submit(Submit {
        queue: QueueKind::Graphics,
        commands: list,
        wait: &[],
        signal: &[],
    })?;
    Ok(device.wait_idle()?)
}

/// `size` bytes of `buffer`, copied to a readback buffer.
fn download(device: &dyn Device, buffer: Buffer, size: u64) -> Result<Vec<u8>, RenderError> {
    let readback = device.create_buffer(&BufferDesc {
        size: NonZeroU64::new(size.max(16)).unwrap_or(NonZeroU64::MIN),
        usage: BufferUsage::COPY_DST,
        memory: MemoryLocation::Readback,
        label: "gi readback".into(),
    })?;
    let mut list = CommandList::new("gi readback");
    list.push(Command::CopyBuffer {
        src: buffer,
        src_offset: 0,
        dst: readback,
        dst_offset: 0,
        size,
    });
    submit(device, &list)?;
    let out = device.read_buffer(readback, 0, size)?;
    device.destroy_buffer(readback);
    Ok(out)
}

/// GI's scene as acceleration structures: one bottom level per proxy (its triangles in the
/// software BVH's order, so triangle indices agree) and a top level of the placed instances
/// (instance ids = their indices).
#[derive(Debug, Default)]
struct HardwareScene {
    bottoms: Vec<(AccelerationStructure, Buffer)>,
    top: Option<AccelerationStructure>,
    mesh_version: Option<u64>,
    placed: Vec<(u32, [[f32; 4]; 3])>,
}

impl HardwareScene {
    /// Rebuilds what changed and returns the top level's bindless index.
    fn sync(&mut self, device: &dyn Device, scene: &GiScene) -> Result<u32, RenderError> {
        if self.mesh_version != Some(scene.mesh_version()) {
            self.clear(device);
            for (k, mesh) in scene.accel().meshes().iter().enumerate() {
                self.bottoms.push(bottom(device, mesh.triangles(), k)?);
            }
            self.mesh_version = Some(scene.mesh_version());
        }
        let placed: Vec<(u32, [[f32; 4]; 3])> = scene
            .accel()
            .instances()
            .iter()
            .map(|i| (i.mesh, rows3x4(&i.world)))
            .collect();
        if self.top.is_none() || placed != self.placed {
            if let Some(top) = self.top.take() {
                device.destroy_acceleration_structure(top);
            }
            let instances: Vec<AccelInstance> = placed
                .iter()
                .enumerate()
                .filter_map(|(k, (mesh, transform))| {
                    self.bottoms
                        .get(*mesh as usize)
                        .map(|(blas, _)| AccelInstance {
                            geometry: *blas,
                            transform: *transform,
                            id: k as u32,
                            mask: 0xff,
                        })
                })
                .collect();
            self.top = Some(
                device
                    .build_acceleration_structure(&AccelDesc::Instances(&instances), "gi scene")?,
            );
            self.placed = placed;
        }
        let top = self
            .top
            .ok_or_else(|| RenderError::Shader("GI's top level wasn't built".into()))?;
        Ok(device.accel_index(top)?.0)
    }

    /// Frees every structure and vertex buffer (the top level first).
    fn clear(&mut self, device: &dyn Device) {
        if let Some(top) = self.top.take() {
            device.destroy_acceleration_structure(top);
        }
        for (blas, buffer) in self.bottoms.drain(..) {
            device.destroy_acceleration_structure(blas);
            device.destroy_buffer(buffer);
        }
        self.placed.clear();
    }
}

/// A bottom level over `triangles` (a GpuOnly copy of their corners).
fn bottom(
    device: &dyn Device,
    triangles: &[concordia_bvh::Triangle],
    k: usize,
) -> Result<(AccelerationStructure, Buffer), RenderError> {
    let bytes: Vec<u8> = triangles
        .iter()
        .flat_map(|t| t.v)
        .flat_map(|v| [v.x, v.y, v.z])
        .flat_map(f32::to_le_bytes)
        .collect();
    let buffer = device.create_buffer(&BufferDesc {
        size: NonZeroU64::new(bytes.len().max(16) as u64).unwrap_or(NonZeroU64::MIN),
        usage: BufferUsage::ACCEL_INPUT | BufferUsage::STORAGE,
        memory: MemoryLocation::Upload,
        label: format!("gi proxy {k}"),
    })?;
    device.write_buffer(buffer, 0, &bytes)?;
    let count = triangles.len().max(1) as u32;
    let geometry = [TriangleGeometry {
        vertices: buffer,
        vertex_offset: 0,
        vertex_stride: 12,
        vertex_count: count * 3,
        indices: None,
        triangle_count: triangles.len() as u32,
    }];
    let blas = device.build_acceleration_structure(&AccelDesc::Triangles(&geometry), "gi proxy")?;
    Ok((blas, buffer))
}

/// Object → world as three rows of a 3 × 4 matrix.
fn rows3x4(m: &concordia_core::math::Mat4) -> [[f32; 4]; 3] {
    let row = |r: usize| [m.x_axis[r], m.y_axis[r], m.z_axis[r], m.w_axis[r]];
    [row(0), row(1), row(2)]
}
