//! The GPU backend: the same algorithm as the CPU one, in Slang compute (`shaders/pathtrace.slang`),
//! over the scene packed into bindless buffers of `float4`/`uint4` rows (identical layout on every
//! backend). Samples run in batches so no dispatch is long enough for a GPU watchdog.

use std::path::PathBuf;
use std::sync::Arc;

use concordia_assets::AlphaMode;
use concordia_core::math::{Mat3, Mat4};
use concordia_material::rows::{NO_TEXTURE, Row, material_rows};
use concordia_render_types::Projection;
use concordia_rhi::{
    Buffer, BufferDesc, BufferUsage, Command, CommandList, ComputePipelineDesc, Device,
    MemoryLocation, Pipeline, QueueKind, Submit,
};
use concordia_shaders::{CompileRequest, ShaderCompiler, Stage, Target};

use crate::PathTraceError;
use crate::cpu::{HdrImage, MAX_SIZE, Settings};
use crate::scene::{Light, Scene};

/// Samples per dispatch.
const BATCH: u32 = 16;
/// Header rows.
const HEADER_ROWS: usize = 13;

/// The folder holding `pathtrace.slang`.
pub fn shader_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders"))
}

/// The GPU tracer: a device and its compiled kernel.
pub struct GpuTracer {
    device: Arc<dyn Device>,
    pipeline: Pipeline,
}

impl std::fmt::Debug for GpuTracer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuTracer").finish_non_exhaustive()
    }
}

fn gpu(e: impl std::fmt::Display) -> PathTraceError {
    PathTraceError::Gpu(e.to_string())
}

impl GpuTracer {
    /// Compiles the kernel for `device` (it needs bindless buffers).
    pub fn new(
        device: Arc<dyn Device>,
        compiler: &dyn ShaderCompiler,
    ) -> Result<Self, PathTraceError> {
        if !device.caps().bindless {
            return Err(gpu("the device has no bindless buffers"));
        }
        let module = compiler
            .compile(&CompileRequest {
                path: shader_dir().join("pathtrace.slang"),
                entry: "trace_pixels".into(),
                stage: Stage::Compute,
                target: Target::from(device.caps().backend),
                defines: Default::default(),
                include_dirs: vec![
                    concordia_shaders::builtin_dir(),
                    concordia_material::shader_dir(),
                ],
            })
            .map_err(gpu)?;
        let pipeline = device
            .create_compute_pipeline(&ComputePipelineDesc {
                shader: module.source(),
                threads_per_group: module.reflection.threads_per_group.unwrap_or([8, 8, 1]),
                label: "path tracer".into(),
            })
            .map_err(gpu)?;
        Ok(Self { device, pipeline })
    }

    /// Renders `scene` on the GPU.
    pub fn render(&self, scene: &Scene, settings: &Settings) -> Result<HdrImage, PathTraceError> {
        validate(settings)?;
        let device = self.device.as_ref();
        let packed = Packed::of(scene)?;
        let mut owned = Vec::new();
        let mut upload = |rows: &[u8]| -> Result<u32, PathTraceError> {
            let b = upload(device, rows)?;
            let index = device.buffer_index(b).map_err(gpu)?.0;
            owned.push(b);
            Ok(index)
        };
        let ids = [
            &packed.nodes,
            &packed.order,
            &packed.tris,
            &packed.normals,
            &packed.meshes,
            &packed.instances,
            &packed.mtab,
            &packed.mrows,
            &packed.lights,
            &packed.emitters,
            &packed.ecdf,
            &packed.tables,
            &packed.env,
            &packed.erows,
            &packed.ecols,
            &packed.uvs,
            &packed.ttab,
            &packed.texels,
        ]
        .into_iter()
        .map(|bytes| upload(bytes))
        .collect::<Result<Vec<u32>, _>>()?;
        let header = upload(&bytes_of(&packed.header(&ids, scene, settings)))?;
        let pixels = settings.width as usize * settings.height as usize;
        let out = upload(&vec![0u8; pixels * 16])?;
        let out_buffer = *owned.last().ok_or_else(|| gpu("no output buffer"))?;
        let groups = [settings.width.div_ceil(8), settings.height.div_ceil(8), 1];
        let mut first = 0;
        while first < settings.spp {
            let count = BATCH.min(settings.spp - first);
            let mut list = CommandList::new("path trace");
            list.push(Command::SetPipeline(self.pipeline))
                .push(push(&[header, first, count, out]))
                .push(Command::Dispatch { groups });
            submit(device, &list)?;
            first += count;
        }
        let sums = download(device, out_buffer, (pixels * 16) as u64)?;
        for b in owned {
            device.destroy_buffer(b);
        }
        let scale = scene.camera.exposure / settings.spp as f32;
        let pixels = sums
            .as_chunks::<16>()
            .0
            .iter()
            .map(|p| {
                [0, 4, 8].map(|o| f32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]) * scale)
            })
            .collect();
        Ok(HdrImage {
            width: settings.width,
            height: settings.height,
            pixels,
        })
    }
}

fn validate(s: &Settings) -> Result<(), PathTraceError> {
    let bad = |m: String| Err(PathTraceError::Settings(m));
    if !(1..=MAX_SIZE).contains(&s.width) || !(1..=MAX_SIZE).contains(&s.height) {
        return bad(format!(
            "size {}×{} must be 1..={MAX_SIZE}",
            s.width, s.height
        ));
    }
    if s.spp == 0 || s.max_bounces == 0 {
        return bad("spp and max_bounces must be at least 1".into());
    }
    Ok(())
}

/// The scene as row buffers (bytes), in the kernel's layout.
#[derive(Debug, Default)]
struct Packed {
    nodes: Vec<u8>,
    order: Vec<u8>,
    tris: Vec<u8>,
    normals: Vec<u8>,
    meshes: Vec<u8>,
    instances: Vec<u8>,
    mtab: Vec<u8>,
    mrows: Vec<u8>,
    lights: Vec<u8>,
    emitters: Vec<u8>,
    ecdf: Vec<u8>,
    tables: Vec<u8>,
    env: Vec<u8>,
    erows: Vec<u8>,
    ecols: Vec<u8>,
    /// Corner texture coordinates, parallel to `tris`.
    uvs: Vec<u8>,
    /// Per texture: first texel, width, height.
    ttab: Vec<u8>,
    /// Every texture's linear RGBA texels, rows from the top.
    texels: Vec<u8>,
    tlas_node: u32,
    tlas_order: u32,
}

fn f4(v: [f32; 4]) -> Row {
    v.map(f32::to_bits)
}

fn bytes_of(rows: &[Row]) -> Vec<u8> {
    rows.iter()
        .flatten()
        .flat_map(|w| w.to_le_bytes())
        .collect()
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

impl Packed {
    fn of(scene: &Scene) -> Result<Self, PathTraceError> {
        let (mut nodes, mut order, mut tris, mut normals, mut meshes, mut uvs) = (
            Vec::new(),
            Vec::<u32>::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        for (m, bvh) in scene.accel.meshes().iter().enumerate() {
            let (node_base, tri_base, order_base) = (nodes.len() / 2, tris.len() / 3, order.len());
            meshes.push([node_base as u32, tri_base as u32, order_base as u32, 0]);
            push_tree(&mut nodes, &mut order, bvh.nodes(), bvh.order());
            for t in bvh.triangles() {
                tris.extend(t.v.map(|v| f4([v.x, v.y, v.z, 0.0])));
            }
            for n in &scene.normals[m] {
                normals.extend(n.map(|v| f4([v.x, v.y, v.z, 0.0])));
            }
            for prim in 0..bvh.triangles().len() {
                let corners = scene.uvs[m].get(prim).copied().unwrap_or_default();
                uvs.extend(corners.map(|t| f4([t.x, t.y, 0.0, 0.0])));
            }
        }
        let tlas = scene.accel.tree();
        let (tlas_node, tlas_order) = if tlas.nodes().is_empty() {
            (u32::MAX, 0)
        } else {
            ((nodes.len() / 2) as u32, order.len() as u32)
        };
        push_tree(&mut nodes, &mut order, tlas.nodes(), tlas.order());
        let instances = instance_rows(scene);
        let (mtab, mrows) = material_rows_of(scene)?;
        let emitters: Vec<Row> = scene
            .emitters
            .iter()
            .flat_map(|e| {
                let [a, b, c] = e.v;
                [
                    [a.x.to_bits(), a.y.to_bits(), a.z.to_bits(), e.instance],
                    [b.x.to_bits(), b.y.to_bits(), b.z.to_bits(), e.prim],
                    f4([c.x, c.y, c.z, 0.0]),
                ]
            })
            .collect();
        let (env, erows, ecols) = match &scene.environment {
            Some(env) => {
                let (rows, cols) = env.distributions();
                let texels: Vec<Row> = env
                    .texels()
                    .iter()
                    .map(|t| f4([t.x, t.y, t.z, 0.0]))
                    .collect();
                (bytes_of(&texels), floats(rows), floats(cols))
            }
            None => (Vec::new(), Vec::new(), Vec::new()),
        };
        let (mut ttab, mut texels) = (Vec::new(), Vec::new());
        for t in &scene.textures {
            ttab.push([(texels.len()) as u32, t.width, t.height, 0]);
            texels.extend(t.texels.iter().map(|&c| f4(c)));
        }
        Ok(Self {
            nodes: bytes_of(&nodes),
            order: order.iter().flat_map(|w| w.to_le_bytes()).collect(),
            tris: bytes_of(&tris),
            normals: bytes_of(&normals),
            meshes: bytes_of(&meshes),
            instances: bytes_of(&instances),
            mtab: bytes_of(&mtab),
            mrows: bytes_of(&mrows),
            lights: bytes_of(&light_rows(&scene.lights)),
            emitters: bytes_of(&emitters),
            ecdf: floats(&scene.emitter_cdf),
            tables: bytes_of(&table_rows()),
            env,
            erows,
            ecols,
            uvs: bytes_of(&uvs),
            ttab: bytes_of(&ttab),
            texels: bytes_of(&texels),
            tlas_node,
            tlas_order,
        })
    }

    /// The header: buffer indices, counts, the camera and the settings (`pathtrace.slang` `context`).
    fn header(&self, ids: &[u32], scene: &Scene, s: &Settings) -> [Row; HEADER_ROWS] {
        let cam = &scene.camera;
        let rows = |r: usize| {
            let m = cam.world;
            f4([m.x_axis[r], m.y_axis[r], m.z_axis[r], m.w_axis[r]])
        };
        let (kind, param) = match cam.projection {
            Projection::Perspective { fov_y, .. } => (0, (fov_y / 2.0).tan()),
            Projection::Orthographic { height, .. } => (1, height / 2.0),
        };
        let (env_w, env_h) = scene.environment.as_ref().map_or((1, 1), |e| e.size());
        let flags = u32::from(scene.environment.is_some()) | (u32::from(scene.sky_visible) << 1);
        let area: f32 = scene
            .emitters
            .iter()
            .map(|e| e.area)
            .sum::<f32>()
            .max(f32::MIN_POSITIVE);
        [
            [ids[0], ids[1], ids[2], ids[3]],
            [ids[4], ids[5], ids[6], ids[7]],
            [ids[8], ids[9], ids[10], ids[11]],
            [ids[12], ids[13], ids[14], flags],
            [
                self.tlas_node,
                self.tlas_order,
                scene.lights.len() as u32,
                scene.emitters.len() as u32,
            ],
            [env_w, env_h, s.width, s.height],
            [s.spp, s.seed, s.max_bounces, 0],
            rows(0),
            rows(1),
            rows(2),
            [
                kind,
                param.to_bits(),
                cam.exposure.to_bits(),
                area.to_bits(),
            ],
            f4([cam.clear.x, cam.clear.y, cam.clear.z, 0.0]),
            [ids[15], ids[16], ids[17], scene.textures.len() as u32],
        ]
    }
}

/// Appends `tree`'s nodes (2 rows each: min + first, max + count) and leaf order.
fn push_tree(
    nodes: &mut Vec<Row>,
    order: &mut Vec<u32>,
    tree: &[crate::bvh::Node],
    leaves: &[u32],
) {
    for n in tree {
        nodes.push([
            n.min.x.to_bits(),
            n.min.y.to_bits(),
            n.min.z.to_bits(),
            n.first,
        ]);
        nodes.push([
            n.max.x.to_bits(),
            n.max.y.to_bits(),
            n.max.z.to_bits(),
            n.count,
        ]);
    }
    order.extend_from_slice(leaves);
}

/// Seven rows per instance: world → object (3 rows), the normal matrix (3), mesh and material.
fn instance_rows(scene: &Scene) -> Vec<Row> {
    let mut rows = Vec::new();
    for ((placed, inv), shading) in scene
        .accel
        .instances()
        .iter()
        .zip(scene.accel.inverses())
        .zip(&scene.instances)
    {
        rows.extend((0..3).map(|r| f4(row4(inv, r))));
        rows.extend((0..3).map(|r| f4(row3(&shading.normals, r))));
        rows.push([placed.mesh, shading.material, 0, 0]);
    }
    rows
}

fn row4(m: &Mat4, r: usize) -> [f32; 4] {
    [m.x_axis[r], m.y_axis[r], m.z_axis[r], m.w_axis[r]]
}

fn row3(m: &Mat3, r: usize) -> [f32; 4] {
    [m.x_axis[r], m.y_axis[r], m.z_axis[r], 0.0]
}

/// Six table rows per material, and the shared material rows (no textures).
fn material_rows_of(scene: &Scene) -> Result<(Vec<Row>, Vec<Row>), PathTraceError> {
    let (mut table, mut rows) = (Vec::new(), Vec::new());
    for m in &scene.materials {
        let encoded = material_rows(&m.source, &m.program, &|r| {
            Some(scene.texture_ids.get(r).copied().unwrap_or(NO_TEXTURE))
        })
        .map_err(PathTraceError::Material)?;
        let (mode, cutoff) = match m.alpha {
            AlphaMode::Opaque => (0, 0.0),
            AlphaMode::Mask { cutoff } => (1, cutoff),
            AlphaMode::Blend => (2, 0.0),
        };
        let shown = m.program.unlit();
        let mut lobes = [0.0f32; 8];
        for (k, a) in m.lobes.iter().take(8).enumerate() {
            lobes[k] = *a;
        }
        table.push([
            rows.len() as u32,
            m.lobes.len().min(8) as u32,
            mode,
            u32::from(m.unlit),
        ]);
        table.push([
            cutoff.to_bits(),
            shown.alpha.to_bits(),
            u32::from(m.textured),
            0,
        ]);
        table.push(f4([m.emission.x, m.emission.y, m.emission.z, 0.0]));
        table.push(f4([shown.color.x, shown.color.y, shown.color.z, 0.0]));
        table.push(f4([lobes[0], lobes[1], lobes[2], lobes[3]]));
        table.push(f4([lobes[4], lobes[5], lobes[6], lobes[7]]));
        rows.extend(encoded);
    }
    Ok((table, rows))
}

/// Four rows per light: position + range, axis or travel + kind, intensity, cone.
fn light_rows(lights: &[Light]) -> Vec<Row> {
    let kind = |k: u32| f32::from_bits(k);
    lights
        .iter()
        .flat_map(|l| match *l {
            Light::Point {
                position,
                intensity,
                range,
                radius,
            } => [
                f4([position.x, position.y, position.z, range]),
                f4([0.0, 0.0, 0.0, kind(0)]),
                f4([intensity.x, intensity.y, intensity.z, radius]),
                f4([0.0, 1.0, 0.0, 0.0]),
            ],
            Light::Spot {
                position,
                axis,
                intensity,
                range,
                cone,
                radius,
            } => [
                f4([position.x, position.y, position.z, range]),
                f4([axis.x, axis.y, axis.z, kind(1)]),
                f4([intensity.x, intensity.y, intensity.z, radius]),
                f4([cone.scale, cone.offset, 0.0, 0.0]),
            ],
            Light::Sun {
                travel,
                illuminance,
                angular_radius,
            } => [
                f4([0.0; 4]),
                f4([travel.x, travel.y, travel.z, kind(2)]),
                f4([illuminance.x, illuminance.y, illuminance.z, angular_radius]),
                f4([0.0; 4]),
            ],
        })
        .collect()
}

/// The direct-lobe (A, B) and sheen albedo tables, one row per (roughness, N·V) texel.
fn table_rows() -> Vec<Row> {
    let (direct, sheen) = (
        concordia_material::dfg::direct_lut_cached(),
        concordia_material::dfg::sheen_lut_cached(),
    );
    direct
        .iter()
        .zip(sheen)
        .map(|(ab, s)| f4([ab[0], ab[1], *s, 0.0]))
        .collect()
}

/// A bindless GPU buffer holding `data` (at least 16 bytes, so every index is valid).
fn upload(device: &dyn Device, data: &[u8]) -> Result<Buffer, PathTraceError> {
    let size = data.len().max(16) as u64;
    let mut bytes = data.to_vec();
    bytes.resize(size as usize, 0);
    let desc = |usage, memory, label: &str| BufferDesc {
        size: std::num::NonZeroU64::new(size).unwrap_or(std::num::NonZeroU64::MIN),
        usage,
        memory,
        label: label.into(),
    };
    let staging = device
        .create_buffer(&desc(
            BufferUsage::COPY_SRC,
            MemoryLocation::Upload,
            "path tracer staging",
        ))
        .map_err(gpu)?;
    device.write_buffer(staging, 0, &bytes).map_err(gpu)?;
    let usage = BufferUsage::STORAGE
        | BufferUsage::BINDLESS
        | BufferUsage::COPY_DST
        | BufferUsage::COPY_SRC;
    let buffer = device
        .create_buffer(&desc(usage, MemoryLocation::GpuOnly, "path tracer data"))
        .map_err(gpu)?;
    let mut list = CommandList::new("path tracer upload");
    list.push(Command::CopyBuffer {
        src: staging,
        src_offset: 0,
        dst: buffer,
        dst_offset: 0,
        size,
    });
    submit(device, &list)?;
    device.destroy_buffer(staging);
    Ok(buffer)
}

fn download(device: &dyn Device, buffer: Buffer, size: u64) -> Result<Vec<u8>, PathTraceError> {
    let readback = device
        .create_buffer(&BufferDesc {
            size: std::num::NonZeroU64::new(size.max(16)).unwrap_or(std::num::NonZeroU64::MIN),
            usage: BufferUsage::COPY_DST,
            memory: MemoryLocation::Readback,
            label: "path tracer readback".into(),
        })
        .map_err(gpu)?;
    let mut list = CommandList::new("path tracer readback");
    list.push(Command::CopyBuffer {
        src: buffer,
        src_offset: 0,
        dst: readback,
        dst_offset: 0,
        size,
    });
    submit(device, &list)?;
    let bytes = device.read_buffer(readback, 0, size).map_err(gpu)?;
    device.destroy_buffer(readback);
    Ok(bytes)
}

fn submit(device: &dyn Device, list: &CommandList) -> Result<(), PathTraceError> {
    device
        .submit(Submit {
            queue: QueueKind::Graphics,
            commands: list,
            wait: &[],
            signal: &[],
        })
        .map_err(gpu)?;
    device.wait_idle().map_err(gpu)
}

fn push(words: &[u32]) -> Command {
    Command::PushConstants(words.iter().flat_map(|w| w.to_le_bytes()).collect())
}
