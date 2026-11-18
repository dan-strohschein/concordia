//! The renderer: forward PBR into HDR, tonemapping into sRGB, readback, scheduled by the graph.

use std::collections::BTreeMap;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;

use concordia_assets::{AssetRef, MaterialData};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_rhi::{
    Access, AccessFlags, Buffer, BufferDesc, BufferUsage, ClearValue, ColorAttachment, Command,
    CommandList, DepthAttachment, Device, HeapDesc, Layout, LoadOp, MemoryLocation, Pipeline,
    QueueKind, RenderPassDesc, SampleCount, Stages, StoreOp, Submit, Texture, TextureDesc,
    TextureUsage,
};
use concordia_shaders::ShaderCompiler;

use crate::assets::AssetSource;
use crate::camera::{eye, view_matrix};
use crate::cluster::{ClusterBuffers, ClusterCuller, ClusterGrid, ClusterSettings};
use crate::frame::{
    DrawPush, FrameParams, frame_block, instance_block, material_block, material_textures,
};
use crate::gpu::{GpuAssets, GpuMesh, Uploads};
use crate::graph::{GraphTexture, PassContext, RenderGraph, Use};
use crate::lighting::punctual_lights;
use crate::pipelines::{DEPTH, HDR, OUTPUT, Pipelines};
use crate::{CameraView, Msaa, RenderError, RenderWorld, Tonemap};

/// The largest capture side.
pub const MAX_SIZE: u32 = 8192;

/// What to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderRequest {
    /// Camera entity, or the highest-priority camera.
    pub camera: Option<EntityId>,
    /// Width in pixels (1..=8192).
    pub width: u32,
    /// Height in pixels (1..=8192).
    pub height: u32,
}

/// A rendered image: sRGB-encoded RGBA8, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Pixels.
    pub rgba: Vec<u8>,
}

/// Renders `RenderWorld`s offscreen on one device; meshes and textures stay cached between frames.
pub struct Renderer {
    device: Arc<dyn Device>,
    pipelines: Pipelines,
    culler: ClusterCuller,
    clusters: ClusterSettings,
    stats: RenderStats,
    assets: GpuAssets,
    frames: u64,
}

/// Where a draw item goes this frame.
struct DrawOrder {
    item: usize,
    material: u32,
    double_sided: bool,
}

/// Per-frame instance and material bytes, and the draw order.
struct FrameBlocks {
    instances: Vec<u8>,
    materials: Vec<u8>,
    order: Vec<DrawOrder>,
}

/// The frame's light clustering, recorded before the graph.
struct ClusterJob<'a> {
    buffers: &'a ClusterBuffers,
    grid: &'a ClusterGrid,
    light_count: usize,
}

/// Light and overflow counts of the last frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Point and spot lights in the world.
    pub punctual_lights: usize,
    /// Lights beyond the frame budget (not drawn).
    pub dropped_lights: usize,
    /// Cluster assignments dropped because a cluster was full (lights missing there).
    pub cluster_overflow: u32,
}

/// One draw, ready to record.
struct Draw {
    pipeline: Pipeline,
    push: Vec<u8>,
    vertices: u32,
}

impl Renderer {
    /// Compiles the shaders and creates the pipelines.
    pub fn new(
        device: Arc<dyn Device>,
        compiler: Box<dyn ShaderCompiler>,
    ) -> Result<Self, RenderError> {
        let pipelines = Pipelines::new(device.as_ref(), compiler.as_ref())?;
        let culler = ClusterCuller::new(device.clone(), compiler.as_ref())?;
        Ok(Self {
            device,
            pipelines,
            culler,
            clusters: ClusterSettings::default(),
            stats: RenderStats::default(),
            assets: GpuAssets::default(),
            frames: 0,
        })
    }

    /// Frames rendered so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Renders `request` and reads the image back.
    pub fn render(
        &mut self,
        world: &RenderWorld,
        assets: &mut dyn AssetSource,
        request: &RenderRequest,
    ) -> Result<Image, RenderError> {
        let (width, height) = check_size(request)?;
        let view = world.camera(request.camera)?;
        let aspect = width.get() as f32 / height.get() as f32;
        let grid = ClusterGrid::new(self.clusters, &view.camera.projection, aspect)?;
        let mut frame = FrameResources::default();
        let result = self.render_frame(world, assets, view, &grid, (width, height), &mut frame);
        frame.release(self.device.as_ref());
        self.frames += 1;
        result
    }

    /// Light and overflow counts of the last frame.
    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    fn render_frame(
        &mut self,
        world: &RenderWorld,
        assets: &mut dyn AssetSource,
        view: &CameraView,
        grid: &ClusterGrid,
        (width, height): (NonZeroU32, NonZeroU32),
        frame: &mut FrameResources,
    ) -> Result<Image, RenderError> {
        let aspect = width.get() as f32 / height.get() as f32;
        let view_m = view_matrix(&view.world);
        let view_proj = view.camera.projection.matrix(aspect)? * view_m;
        let samples = match view.camera.msaa {
            Msaa::Off => SampleCount::X1,
            Msaa::X4 => SampleCount::X4,
        };
        let mut up = Uploads::new();
        let materials = self.load_assets(world, assets, &mut up)?;
        let device = self.device.clone();
        let device = device.as_ref();
        let exposure = view.camera.exposure();
        let (dir, radiance) = match world.lights.first() {
            Some(l) => (
                l.direction,
                Vec3::from_array(l.light.color) * l.light.illuminance_lux * exposure,
            ),
            None => (Vec3::NEG_Y, Vec3::ZERO),
        };
        let lights = punctual_lights(
            world,
            &view_m,
            exposure,
            grid.settings().max_lights as usize,
        );
        if lights.dropped > 0 {
            tracing::warn!(target: "concordia::render", dropped = lights.dropped, "more punctual lights than the frame budget");
        }
        let light_buf = frame.keep(up.buffer(device, &lights.rows, "lights")?);
        let cluster = self.culler.prepare(&mut up, grid, &lights.bounds)?;
        cluster.buffers().iter().for_each(|b| {
            frame.keep(*b);
        });
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        let block = frame_block(&FrameParams {
            view_proj,
            view: view_m,
            eye: eye(&view.world),
            sun_dir: dir,
            sun_radiance: radiance,
            grid,
            size: (width.get(), height.get()),
            light_count: lights.bounds.len() as u32,
            buffers: [
                index(light_buf)?,
                cluster.counts_index,
                cluster.indices_index,
            ],
        });
        let frame_buf = frame.keep(up.buffer(device, &block, "frame")?);
        let FrameBlocks {
            instances,
            materials: material_bytes,
            order,
        } = self.instance_data(world, &materials);
        let inst_buf = frame.keep(up.buffer(device, &instances, "instances")?);
        let mat_buf = frame.keep(up.buffer(device, &material_bytes, "materials")?);
        up.flush(device)?;
        let (frame_i, inst_i, mat_i) = (index(frame_buf)?, index(inst_buf)?, index(mat_buf)?);
        let mut draws = Vec::with_capacity(order.len());
        for (
            instance,
            DrawOrder {
                item,
                material: material_index,
                double_sided,
            },
        ) in order.into_iter().enumerate()
        {
            let mesh = self.assets.meshes[&world.draws[item].mesh];
            let push = DrawPush {
                frame: frame_i,
                instances: inst_i,
                instance: instance as u32,
                materials: mat_i,
                material: material_index,
                indices: mesh.indices,
                positions: mesh.positions,
                normals: mesh.normals,
                tangents: mesh.tangents,
                uvs: mesh.uvs,
            };
            let pipeline = self.pipelines.forward(device, samples, double_sided)?;
            draws.push(Draw {
                pipeline,
                push: push.bytes(),
                vertices: mesh.count,
            });
        }
        let job = ClusterJob {
            buffers: &cluster,
            grid,
            light_count: lights.bounds.len(),
        };
        let (image, overflow) = self.execute(view, draws, samples, (width, height), frame, &job)?;
        self.stats = RenderStats {
            punctual_lights: world.punctual.len(),
            dropped_lights: lights.dropped,
            cluster_overflow: overflow,
        };
        if overflow > 0 {
            tracing::warn!(target: "concordia::render", overflow, "clusters overflowed; raise max_per_cluster or reduce light ranges");
        }
        Ok(image)
    }

    /// Uploads missing meshes and textures; returns this frame's materials (deduplicated, in order).
    fn load_assets(
        &mut self,
        world: &RenderWorld,
        assets: &mut dyn AssetSource,
        up: &mut Uploads,
    ) -> Result<BTreeMap<AssetRef, MaterialData>, RenderError> {
        let device = self.device.as_ref();
        let fail = |r: &AssetRef, message: String| RenderError::Asset {
            reference: r.to_string(),
            message,
        };
        let mut materials = BTreeMap::new();
        for item in &world.draws {
            if !self.assets.meshes.contains_key(&item.mesh) {
                let data = assets.mesh(&item.mesh).map_err(|m| fail(&item.mesh, m))?;
                let mesh = GpuMesh::upload(device, up, &data, &item.mesh.to_string())?;
                self.assets.meshes.insert(item.mesh.clone(), mesh);
            }
            if !materials.contains_key(&item.material) {
                let material = assets
                    .material(&item.material)
                    .map_err(|m| fail(&item.material, m))?;
                for t in material_textures(&material) {
                    if !self.assets.textures.contains_key(t) {
                        let data = assets.texture(t).map_err(|m| fail(t, m))?;
                        let texture = up.texture(device, &data, &t.to_string())?;
                        self.assets
                            .textures
                            .insert(t.clone(), (texture, device.texture_index(texture)?.0));
                    }
                }
                materials.insert(item.material.clone(), material);
            }
        }
        Ok(materials)
    }

    /// Instance and material blocks plus the draw order (culled pipelines first, then entity id).
    fn instance_data(
        &self,
        world: &RenderWorld,
        materials: &BTreeMap<AssetRef, MaterialData>,
    ) -> FrameBlocks {
        let textures: BTreeMap<AssetRef, u32> = self
            .assets
            .textures
            .iter()
            .map(|(r, (_, i))| (r.clone(), *i))
            .collect();
        let indices: BTreeMap<&AssetRef, u32> = materials
            .keys()
            .enumerate()
            .map(|(i, r)| (r, i as u32))
            .collect();
        let mut material_bytes = Vec::new();
        materials
            .values()
            .for_each(|m| material_block(&mut material_bytes, m, &textures));
        let mut order: Vec<DrawOrder> = world
            .draws
            .iter()
            .enumerate()
            .map(|(item, d)| DrawOrder {
                item,
                material: indices[&d.material],
                double_sided: materials[&d.material].double_sided,
            })
            .collect();
        order.sort_by_key(|d| (d.double_sided, d.item));
        let mut instances = Vec::new();
        order
            .iter()
            .for_each(|d| instance_block(&mut instances, &world.draws[d.item].world));
        FrameBlocks {
            instances,
            materials: material_bytes,
            order,
        }
    }

    /// Builds the frame graph, places transients, records and submits, then reads back.
    fn execute(
        &mut self,
        view: &CameraView,
        draws: Vec<Draw>,
        samples: SampleCount,
        size: (NonZeroU32, NonZeroU32),
        frame: &mut FrameResources,
        job: &ClusterJob<'_>,
    ) -> Result<(Image, u32), RenderError> {
        let device = self.device.as_ref();
        let (w, h) = (size.0.get(), size.1.get());
        let clusters = job.grid.len() as u64;
        let overflow_rb = frame.keep(device.create_buffer(&BufferDesc {
            size: NonZeroU64::new(clusters * 4).unwrap_or(NonZeroU64::MIN),
            usage: BufferUsage::COPY_DST,
            memory: MemoryLocation::Readback,
            label: "cluster overflow".into(),
        })?);
        let output = frame.texture(device.create_texture(&desc(
            size,
            OUTPUT,
            SampleCount::X1,
            TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC,
            "output",
        ))?);
        let readback = frame.keep(device.create_buffer(&BufferDesc {
            size: NonZeroU64::new(u64::from(w) * u64::from(h) * 4).unwrap_or(NonZeroU64::MIN),
            usage: BufferUsage::COPY_DST,
            memory: MemoryLocation::Readback,
            label: "readback".into(),
        })?);
        let mut graph = RenderGraph::new();
        let out = graph.import_texture(
            "output",
            output,
            desc(
                size,
                OUTPUT,
                SampleCount::X1,
                TextureUsage::RENDER_TARGET,
                "output",
            ),
            Layout::Undefined,
            Some(Layout::CopySrc),
        );
        let targets = add_forward(&mut graph, view, draws, samples, size);
        add_tonemap(
            &mut graph,
            targets.hdr,
            out,
            self.pipelines.tonemap(),
            view.camera.tonemap,
            self.device.clone(),
        );
        graph.add_pass_recorded(
            "readback",
            |p| {
                p.uses(out, Use::CopySrc);
                p.side_effect();
            },
            Some(Box::new(move |ctx: &mut PassContext<'_>| {
                ctx.commands.push(Command::CopyTextureToBuffer {
                    src: ctx.texture(out),
                    mip: 0,
                    dst: readback,
                    offset: 0,
                    bytes_per_row: w * 4,
                });
                Ok(())
            })),
        );
        let compiled =
            graph.compile(&|d: &TextureDesc| device.texture_heap_footprint(d).unwrap_or((0, 1)))?;
        let transients = frame.place(device, &graph, &compiled)?;
        let mut list = CommandList::new("frame");
        let fragment_read = Access {
            stages: Stages::FRAGMENT,
            access: AccessFlags::SHADER_READ,
            layout: Layout::General,
        };
        self.culler.record(
            &mut list,
            job.buffers,
            job.grid,
            job.light_count,
            fragment_read,
        );
        compiled.execute(&graph, &transients, &mut list)?;
        list.push(Command::CopyBuffer {
            src: job.buffers.counts,
            src_offset: clusters * 4,
            dst: overflow_rb,
            dst_offset: 0,
            size: clusters * 4,
        });
        device.submit(Submit {
            queue: QueueKind::Graphics,
            commands: &list,
            wait: &[],
            signal: &[],
        })?;
        device.wait_idle()?;
        let rgba = device.read_buffer(readback, 0, u64::from(w) * u64::from(h) * 4)?;
        let dropped = device.read_buffer(overflow_rb, 0, clusters * 4)?;
        let overflow = dropped
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .sum();
        Ok((
            Image {
                width: w,
                height: h,
                rgba,
            },
            overflow,
        ))
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = self.device.wait_idle();
        self.assets.destroy(self.device.as_ref());
    }
}

fn check_size(request: &RenderRequest) -> Result<(NonZeroU32, NonZeroU32), RenderError> {
    let bad = || RenderError::InvalidSize {
        width: request.width,
        height: request.height,
        max: MAX_SIZE,
    };
    if request.width > MAX_SIZE || request.height > MAX_SIZE {
        return Err(bad());
    }
    Ok((
        NonZeroU32::new(request.width).ok_or_else(bad)?,
        NonZeroU32::new(request.height).ok_or_else(bad)?,
    ))
}

fn desc(
    size: (NonZeroU32, NonZeroU32),
    format: concordia_rhi::Format,
    samples: SampleCount,
    usage: TextureUsage,
    label: &str,
) -> TextureDesc {
    TextureDesc {
        width: size.0,
        height: size.1,
        mip_levels: NonZeroU32::MIN,
        samples,
        format,
        usage,
        label: label.into(),
    }
}

/// The forward pass's graph textures.
struct ForwardTargets {
    hdr: GraphTexture,
}

/// Forward pass: draws into (multisampled) HDR + depth, resolving into single-sample HDR.
fn add_forward(
    graph: &mut RenderGraph,
    view: &CameraView,
    draws: Vec<Draw>,
    samples: SampleCount,
    size: (NonZeroU32, NonZeroU32),
) -> ForwardTargets {
    let hdr = graph.create_texture(desc(
        size,
        HDR,
        SampleCount::X1,
        TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED | TextureUsage::BINDLESS,
        "hdr",
    ));
    let depth = graph.create_texture(desc(
        size,
        DEPTH,
        samples,
        TextureUsage::RENDER_TARGET,
        "depth",
    ));
    let msaa = (samples != SampleCount::X1).then(|| {
        graph.create_texture(desc(
            size,
            HDR,
            samples,
            TextureUsage::RENDER_TARGET,
            "hdr_msaa",
        ))
    });
    let clear = view.camera.clear;
    graph.add_pass_recorded(
        "forward",
        |p| {
            p.uses(depth, Use::DepthWrite);
            p.uses(hdr, Use::ColorWrite);
            if let Some(m) = msaa {
                p.uses(m, Use::ColorWrite);
            }
        },
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            let (target, resolve) = match msaa {
                Some(m) => (ctx.texture(m), Some(ctx.texture(hdr))),
                None => (ctx.texture(hdr), None),
            };
            let store = if resolve.is_some() {
                StoreOp::DontCare
            } else {
                StoreOp::Store
            };
            ctx.commands.push(Command::BeginRenderPass(RenderPassDesc {
                colors: vec![ColorAttachment {
                    texture: target,
                    resolve,
                    load: LoadOp::Clear(ClearValue::Color(clear)),
                    store,
                }],
                depth: Some(DepthAttachment {
                    texture: ctx.texture(depth),
                    load: LoadOp::Clear(ClearValue::Depth(0.0)),
                    store: StoreOp::DontCare,
                }),
            }));
            let mut bound = None;
            for draw in &draws {
                if bound != Some(draw.pipeline) {
                    ctx.commands.push(Command::SetPipeline(draw.pipeline));
                    bound = Some(draw.pipeline);
                }
                ctx.commands.push(Command::PushConstants(draw.push.clone()));
                ctx.commands.push(Command::Draw {
                    vertices: 0..draw.vertices,
                    instances: 0..1,
                });
            }
            ctx.commands.push(Command::EndRenderPass);
            Ok(())
        })),
    );
    ForwardTargets { hdr }
}

/// Tonemap pass: HDR (read by bindless index) → sRGB output.
fn add_tonemap(
    graph: &mut RenderGraph,
    hdr: GraphTexture,
    out: GraphTexture,
    pipeline: Pipeline,
    tonemap: Tonemap,
    device: Arc<dyn Device>,
) {
    let mode = tonemap.shader_mode();
    graph.add_pass_recorded(
        "tonemap",
        |p| {
            p.uses(hdr, Use::SampledRead);
            p.uses(out, Use::ColorWrite);
        },
        Some(Box::new(move |ctx: &mut PassContext<'_>| {
            let index = device.texture_index(ctx.texture(hdr))?.0;
            ctx.commands.push(Command::BeginRenderPass(RenderPassDesc {
                colors: vec![ColorAttachment {
                    texture: ctx.texture(out),
                    resolve: None,
                    load: LoadOp::DontCare,
                    store: StoreOp::Store,
                }],
                depth: None,
            }));
            ctx.commands.push(Command::SetPipeline(pipeline));
            let mut push = index.to_le_bytes().to_vec();
            push.extend(mode.to_le_bytes());
            ctx.commands.push(Command::PushConstants(push));
            ctx.commands.push(Command::Draw {
                vertices: 0..3,
                instances: 0..1,
            });
            ctx.commands.push(Command::EndRenderPass);
            Ok(())
        })),
    );
}

/// Resources that live for one frame: buffers, textures and the transient heap.
#[derive(Default)]
struct FrameResources {
    buffers: Vec<Buffer>,
    textures: Vec<Texture>,
    heap: Option<concordia_rhi::Heap>,
}

impl FrameResources {
    fn keep(&mut self, buffer: Buffer) -> Buffer {
        self.buffers.push(buffer);
        buffer
    }

    fn texture(&mut self, texture: Texture) -> Texture {
        self.textures.push(texture);
        texture
    }

    /// One heap sized by the graph's memory plan; each transient placed at its offset.
    fn place(
        &mut self,
        device: &dyn Device,
        graph: &RenderGraph,
        compiled: &crate::graph::CompiledGraph,
    ) -> Result<BTreeMap<GraphTexture, Texture>, RenderError> {
        let Some(size) = NonZeroU64::new(compiled.heap_size()) else {
            return Ok(BTreeMap::new());
        };
        let heap = device.create_heap(&HeapDesc {
            size,
            label: "frame transients".into(),
        })?;
        self.heap = Some(heap);
        let mut placed = BTreeMap::new();
        for (t, placement) in compiled.placements() {
            let texture =
                device.create_placed_texture(heap, placement.offset, graph.texture_desc(*t))?;
            placed.insert(*t, self.texture(texture));
        }
        Ok(placed)
    }

    fn release(self, device: &dyn Device) {
        let _ = device.wait_idle();
        self.textures
            .into_iter()
            .for_each(|t| device.destroy_texture(t));
        self.buffers
            .into_iter()
            .for_each(|b| device.destroy_buffer(b));
        if let Some(heap) = self.heap {
            device.destroy_heap(heap);
        }
    }
}
