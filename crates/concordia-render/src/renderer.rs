//! The renderer: forward PBR into HDR, tonemapping into sRGB, readback, scheduled by the graph.

use std::collections::BTreeMap;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;

use concordia_assets::{AlphaMode, AssetRef, MaterialData};
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Vec3};
use concordia_rhi::{
    Access, AccessFlags, Buffer, BufferDesc, BufferUsage, ClearValue, ColorAttachment, Command,
    CommandList, DepthAttachment, Device, HeapDesc, Layout, LoadOp, MemoryLocation, Pipeline,
    QueueKind, RenderPassDesc, SampleCount, Stages, StoreOp, Submit, Surface, SurfaceTarget,
    Texture, TextureDesc, TextureUsage,
};
use concordia_shaders::ShaderCompiler;

use crate::assets::{AssetSource, MemoryAssets};
use crate::camera::{eye, view_matrix};
use crate::cluster::{ClusterBuffers, ClusterCuller, ClusterGrid, ClusterSettings};
use crate::environment::EnvironmentCache;
use crate::frame::{DrawPush, FrameParams, IblParams, frame_block, instance_block, material_block};
use crate::gpu::{GpuAssets, GpuMesh, SharedPool, Uploads};
use crate::graph::{GraphTexture, PassContext, RenderGraph, Use};
use crate::lighting::punctual_lights;
use crate::pipelines::{DEPTH, HDR, OUTPUT, Pipelines};
use crate::shadow::{Cascade, CascadeSettings, fit_cascades};
use crate::shadow_pass::{
    ShadowDraw, ShadowJob, ShadowMaps, ShadowPush, add_shadow_passes, world_bounds,
};
use crate::{CameraView, Msaa, RenderError, RenderWorld, Tonemap};

/// The largest capture side.
pub const MAX_SIZE: u32 = 8192;

/// What to render.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderRequest {
    /// Camera entity, or the highest-priority camera.
    pub camera: Option<EntityId>,
    /// Render from this viewpoint instead (the editor's camera); exposure, tonemap and clear
    /// color still come from the scene's camera when there is one.
    pub view: Option<crate::View>,
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
    surfaces: Vec<Surface>,
    /// Called when the frame being rendered reaches the display (set by `present_with`).
    on_presented: Option<concordia_rhi::PresentedCallback>,
    environments: EnvironmentCache,
    shadow_maps: ShadowMaps,
    clusters: ClusterSettings,
    stats: RenderStats,
    assets: GpuAssets,
    frames: u64,
    /// Signalled with a frame's number when the GPU finishes a presented frame.
    timeline: Option<concordia_rhi::Timeline>,
    in_flight: std::collections::VecDeque<InFlight>,
    frames_in_flight: usize,
    /// Transient heaps of retired frames, reused by later ones.
    recycle: Recycler,
}

/// A presented frame the GPU may still be working on.
struct InFlight {
    /// The timeline value its commands signal.
    value: u64,
    resources: FrameResources,
    /// Per-cluster overflow counts, read when the frame retires.
    overflow: Buffer,
    clusters: u64,
}

/// A frame's cluster overflow: read now, or once a presented frame retires.
enum Overflow {
    Read(u32),
    Pending {
        buffer: Buffer,
        clusters: u64,
        value: u64,
    },
}

/// Where a draw item goes this frame.
struct DrawOrder {
    item: usize,
    material: u32,
    double_sided: bool,
    blend: bool,
    /// Inside the camera's view.
    visible: bool,
}

/// Per-frame instance and material bytes, and the draw order.
struct FrameBlocks {
    instances: Vec<u8>,
    materials: Vec<u8>,
    order: Vec<DrawOrder>,
}

/// Everything `execute` records for one frame.
struct FramePlan<'a> {
    draws: Vec<Draw>,
    samples: SampleCount,
    size: (NonZeroU32, NonZeroU32),
    cluster: ClusterJob<'a>,
    shadow: Option<ShadowJob>,
    /// Skybox pipeline and push constants, drawn first in the forward pass.
    sky: Option<(Pipeline, Vec<u8>)>,
    output: Output,
    /// Upload copies (and their barrier), recorded first.
    uploads: CommandList,
}

/// The size and destination of one frame.
struct FrameTarget {
    size: (NonZeroU32, NonZeroU32),
    output: Output,
}

/// Where a frame ends up.
#[derive(Debug, Clone, Copy)]
enum Output {
    /// An owned sRGB texture, copied back to the CPU.
    Readback,
    /// An image acquired from a window surface, presented after the frame.
    Surface {
        surface: Surface,
        texture: Texture,
        format: concordia_rhi::Format,
    },
}

/// Window surfaces present sRGB BGRA (what `CAMetalLayer` offers).
const SURFACE: concordia_rhi::Format = concordia_rhi::Format::Bgra8UnormSrgb;

/// A side-effect pass copying `out` into a new readback buffer.
fn add_readback(
    graph: &mut RenderGraph,
    out: GraphTexture,
    frame: &mut FrameResources,
    device: &dyn Device,
    size: (NonZeroU32, NonZeroU32),
    recycle: &Recycler,
) -> Result<Buffer, RenderError> {
    let (w, h) = (size.0.get(), size.1.get());
    let readback = frame.keep(recycle.buffer(
        device,
        &BufferDesc {
            size: NonZeroU64::new(u64::from(w) * u64::from(h) * 4).unwrap_or(NonZeroU64::MIN),
            usage: BufferUsage::COPY_DST,
            memory: MemoryLocation::Readback,
            label: "readback".into(),
        },
    )?);
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
    Ok(readback)
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
    /// Meshes in view (drawn).
    pub draws: usize,
    /// Meshes outside the camera's view (not drawn by it; they may still cast shadows).
    pub culled: usize,
    /// Forward-pass draw calls (repeated mesh + material pairs are instanced into one).
    pub draw_calls: usize,
    /// GPU buffers, heaps and textures the frame had to create (0 once warm).
    pub allocations: usize,
}

/// One draw call, ready to record: `instances` consecutive instances of one mesh and material.
struct Draw {
    pipeline: Pipeline,
    push: Vec<u8>,
    vertices: u32,
    instances: u32,
    /// What may share the call: the pipeline, material and mesh streams.
    batch: (Pipeline, u32, u32),
    first: u32,
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
            surfaces: Vec::new(),
            on_presented: None,
            environments: EnvironmentCache::default(),
            shadow_maps: ShadowMaps::default(),
            clusters: ClusterSettings::default(),
            stats: RenderStats::default(),
            assets: GpuAssets::default(),
            frames: 0,
            timeline: None,
            in_flight: std::collections::VecDeque::new(),
            frames_in_flight: 2,
            recycle: Recycler::default(),
        })
    }

    /// The most presented frames the GPU may be rendering at once.
    pub const MAX_FRAMES_IN_FLIGHT: usize = 3;

    /// How many presented frames may be in flight (1 = lowest latency; default 2).
    pub fn set_frames_in_flight(&mut self, frames: usize) -> Result<(), RenderError> {
        if !(1..=Self::MAX_FRAMES_IN_FLIGHT).contains(&frames) {
            return Err(RenderError::Gpu(concordia_rhi::RhiError::invalid(
                "frames in flight",
                format!("must be 1..={}, got {frames}", Self::MAX_FRAMES_IN_FLIGHT),
            )));
        }
        self.frames_in_flight = frames;
        Ok(())
    }

    /// Presented frames not yet known to be finished (diagnostics).
    pub fn frames_in_flight(&self) -> usize {
        self.in_flight.len()
    }

    /// GPU memory the device has allocated, if the backend reports it (diagnostics).
    pub fn allocated_bytes(&self) -> Option<u64> {
        self.device.allocated_bytes()
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
        self.frame(world, assets, request, Output::Readback)?
            .ok_or_else(|| {
                RenderError::Gpu(concordia_rhi::RhiError::invalid(
                    "render",
                    "no image was read back",
                ))
            })
    }

    /// Wraps a native window layer for [`Renderer::present`]; the renderer releases it when dropped.
    ///
    /// # Safety
    /// The native object behind `target` must outlive the renderer (or [`Renderer::destroy_surface`]).
    pub unsafe fn create_surface(&mut self, target: SurfaceTarget) -> Result<Surface, RenderError> {
        // SAFETY: forwarded from the caller.
        let surface = unsafe { self.device.create_surface(target, SURFACE) }?;
        self.surfaces.push(surface);
        Ok(surface)
    }

    /// Releases a surface made by [`Renderer::create_surface`].
    pub fn destroy_surface(&mut self, surface: Surface) {
        let _ = self.device.wait_idle();
        self.surfaces.retain(|s| *s != surface);
        self.device.destroy_surface(surface);
    }

    /// Renders `request` into the surface's next image and presents it; `false` when the display
    /// has no free image yet (skip the frame).
    pub fn present(
        &mut self,
        world: &RenderWorld,
        assets: &mut dyn AssetSource,
        request: &RenderRequest,
        surface: Surface,
    ) -> Result<bool, RenderError> {
        self.present_with(world, assets, request, surface, None)
    }

    /// [`Renderer::present`], calling `on_presented` when the frame reaches the display.
    pub fn present_with(
        &mut self,
        world: &RenderWorld,
        assets: &mut dyn AssetSource,
        request: &RenderRequest,
        surface: Surface,
        on_presented: Option<concordia_rhi::PresentedCallback>,
    ) -> Result<bool, RenderError> {
        self.on_presented = on_presented;
        let (width, height) = check_size(request)?;
        let Some(texture) = self.device.acquire(surface, width.get(), height.get())? else {
            return Ok(false);
        };
        match self.frame(
            world,
            assets,
            request,
            Output::Surface {
                surface,
                texture,
                format: SURFACE,
            },
        ) {
            Ok(_) => Ok(true),
            Err(e) => {
                // Hand the image back so the surface is not left holding it.
                let _ = self.device.present(
                    surface,
                    concordia_rhi::Present::new(&CommandList::new("abandoned frame")),
                );
                Err(e)
            }
        }
    }

    fn frame(
        &mut self,
        world: &RenderWorld,
        assets: &mut dyn AssetSource,
        request: &RenderRequest,
        output: Output,
    ) -> Result<Option<Image>, RenderError> {
        let (width, height) = check_size(request)?;
        let editor_view;
        let view = match &request.view {
            Some(v) => {
                editor_view = editor_camera(world, v)?;
                &editor_view
            }
            None => world.camera(request.camera)?,
        };
        let aspect = width.get() as f32 / height.get() as f32;
        let grid = ClusterGrid::new(self.clusters, &view.camera.projection, aspect)?;
        let keep = match output {
            Output::Surface { .. } => self.frames_in_flight - 1,
            Output::Readback => 0,
        };
        self.retire(keep)?;
        let allocated = self.recycle.created();
        let mut frame = FrameResources::default();
        let result = self.render_frame(
            world,
            assets,
            view,
            &grid,
            FrameTarget {
                size: (width, height),
                output,
            },
            &mut frame,
        );
        self.frames += 1;
        self.stats.allocations = self.recycle.created() - allocated;
        match result {
            Ok((
                image,
                Overflow::Pending {
                    buffer,
                    clusters,
                    value,
                },
            )) => {
                self.in_flight.push_back(InFlight {
                    value,
                    resources: frame,
                    overflow: buffer,
                    clusters,
                });
                Ok(image)
            }
            Ok((image, Overflow::Read(_))) => {
                frame.release(self.device.as_ref(), &mut self.recycle);
                Ok(image)
            }
            Err(e) => {
                // A failed frame may have submitted nothing; wait so its resources are safe to free.
                let _ = self.device.wait_idle();
                frame.release(self.device.as_ref(), &mut self.recycle);
                Err(e)
            }
        }
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
        target: FrameTarget,
        frame: &mut FrameResources,
    ) -> Result<(Option<Image>, Overflow), RenderError> {
        let FrameTarget {
            size: (width, height),
            output,
        } = target;
        let aspect = width.get() as f32 / height.get() as f32;
        let view_m = view_matrix(&view.world);
        let view_proj = view.camera.projection.matrix(aspect)? * view_m;
        let samples = match view.camera.msaa {
            Msaa::Off => SampleCount::X1,
            Msaa::X4 => SampleCount::X4,
        };
        let mut up = Uploads::pooled(self.recycle.pool.clone());
        let mut resolved = crate::assets::resolve(world, assets, &|r| self.holds(r))?;
        let materials = self.load_assets(&resolved, &mut up)?;
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
        let shadows = self.prepare_shadows(world, view, aspect, dir)?;
        let environment = match &world.environment {
            Some(env) => {
                let gpu = self
                    .environments
                    .ensure(device, &mut up, &mut resolved, &env.map)?;
                let mut levels = [0u32; 6];
                gpu.levels
                    .iter()
                    .take(6)
                    .enumerate()
                    .for_each(|(i, (_, index))| levels[i] = *index);
                Some((gpu.sh, levels, env.skybox))
            }
            None => None,
        };
        let shadow_params = shadows.as_ref().map(|(cascades, settings, maps)| {
            let mut indices = [0u32; 4];
            maps.iter()
                .take(4)
                .enumerate()
                .for_each(|(i, (_, m))| indices[i] = *m);
            (cascades.as_slice(), settings, indices)
        });
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
            shadow: shadow_params,
            ibl: environment.as_ref().map(|(sh, levels, skybox)| IblParams {
                sh,
                scale: world.environment.as_ref().map_or(0.0, |e| e.intensity) * exposure,
                skybox: *skybox,
                levels: *levels,
            }),
            lut: self.environments.lut_index(),
        });
        let frame_buf = frame.keep(up.buffer(device, &block, "frame")?);
        let FrameBlocks {
            instances,
            materials: material_bytes,
            order,
        } = self.instance_data(
            world,
            &materials,
            &view_m,
            &crate::cull::Frustum::new(view_proj),
        );
        let inst_buf = frame.keep(up.buffer(device, &instances, "instances")?);
        let mat_buf = frame.keep(up.buffer(device, &material_bytes, "materials")?);
        let (uploads, staging) = up.into_commands();
        staging.into_iter().for_each(|b| {
            frame.keep(b);
        });
        let (frame_i, inst_i, mat_i) = (index(frame_buf)?, index(inst_buf)?, index(mat_buf)?);
        let mut draws: Vec<Draw> = Vec::with_capacity(order.len());
        let mut casters: Vec<ShadowDraw> = Vec::new();
        let culled = order.iter().filter(|d| !d.visible).count();
        for (
            instance,
            DrawOrder {
                item,
                material: material_index,
                double_sided,
                blend,
                visible,
            },
        ) in order.into_iter().enumerate()
        {
            let instance = instance as u32;
            let mesh = self.assets.meshes[&world.draws[item].mesh];
            if world.draws[item].cast_shadows {
                match casters.last_mut() {
                    Some(c)
                        if c.push.indices == mesh.indices
                            && c.push.instance + c.instances == instance =>
                    {
                        c.instances += 1;
                    }
                    _ => casters.push(ShadowDraw {
                        push: ShadowPush {
                            frame: frame_i,
                            instances: inst_i,
                            instance,
                            indices: mesh.indices,
                            positions: mesh.positions,
                            cascade: 0,
                        },
                        vertices: mesh.count,
                        instances: 1,
                    }),
                }
            }
            if !visible {
                continue;
            }
            let push = DrawPush {
                frame: frame_i,
                instances: inst_i,
                instance,
                materials: mat_i,
                material: material_index,
                indices: mesh.indices,
                positions: mesh.positions,
                normals: mesh.normals,
                tangents: mesh.tangents,
                uvs: mesh.uvs,
            };
            let pipeline = self
                .pipelines
                .forward(device, samples, double_sided, blend)?;
            let batch = (pipeline, material_index, mesh.indices);
            match draws.last_mut() {
                Some(d) if d.batch == batch && d.first + d.instances == instance => {
                    d.instances += 1
                }
                _ => draws.push(Draw {
                    pipeline,
                    push: push.bytes(),
                    vertices: mesh.count,
                    instances: 1,
                    batch,
                    first: instance,
                }),
            }
        }
        self.stats.draws = world.draws.len() - culled;
        self.stats.culled = culled;
        self.stats.draw_calls = draws.len();
        let job = ClusterJob {
            buffers: &cluster,
            grid,
            light_count: lights.bounds.len(),
        };
        let sky = match environment.as_ref().is_some_and(|(_, _, skybox)| *skybox) {
            true => Some((
                self.pipelines.sky(device, samples)?,
                frame_i.to_le_bytes().to_vec(),
            )),
            false => None,
        };
        let shadow = shadows.map(|(_, settings, maps)| ShadowJob {
            maps,
            resolution: settings.resolution,
            pipeline: self.pipelines.shadow(),
            casters,
        });
        let plan = FramePlan {
            draws,
            samples,
            size: (width, height),
            cluster: job,
            shadow,
            sky,
            output,
            uploads,
        };
        let (image, overflow) = self.execute(view, plan, frame)?;
        self.stats.punctual_lights = world.punctual.len();
        self.stats.dropped_lights = lights.dropped;
        if let Overflow::Read(count) = overflow {
            self.record_overflow(count);
        }
        Ok((image, overflow))
    }

    /// Cascades, settings and shadow maps when the first directional light casts shadows.
    #[allow(clippy::type_complexity)]
    fn prepare_shadows(
        &mut self,
        world: &RenderWorld,
        view: &CameraView,
        aspect: f32,
        sun_dir: Vec3,
    ) -> Result<Option<(Vec<Cascade>, CascadeSettings, Vec<(Texture, u32)>)>, RenderError> {
        let Some(settings) = world.lights.first().and_then(|l| l.light.shadows.clone()) else {
            return Ok(None);
        };
        let casters = world
            .draws
            .iter()
            .filter(|d| d.cast_shadows)
            .map(|d| (self.assets.meshes[&d.mesh].aabb, d.world));
        let bounds = world_bounds(casters);
        let cascades = fit_cascades(
            &settings,
            &view.camera.projection,
            aspect,
            &view.world,
            sun_dir,
            bounds,
        )?;
        let maps = self
            .shadow_maps
            .ensure(self.device.as_ref(), settings.resolution, cascades.len())?
            .to_vec();
        Ok(Some((cascades, settings, maps)))
    }

    /// Uploads missing meshes and textures; returns this frame's materials (deduplicated, in order).
    /// Uploads the meshes and textures of `resolved` (what the GPU lacks) and returns its materials.
    fn load_assets(
        &mut self,
        resolved: &MemoryAssets,
        up: &mut Uploads,
    ) -> Result<BTreeMap<AssetRef, MaterialData>, RenderError> {
        let device = self.device.as_ref();
        for (reference, data) in resolved.meshes() {
            let mesh = GpuMesh::upload(device, up, data, &reference.to_string())?;
            self.assets.meshes.insert(reference.clone(), mesh);
        }
        for (reference, data) in resolved.textures() {
            if self.environments.holds(reference) {
                continue;
            }
            let texture = up.texture(device, data, &reference.to_string())?;
            self.assets.textures.insert(
                reference.clone(),
                (texture, device.texture_index(texture)?.0),
            );
        }
        Ok(resolved
            .materials()
            .map(|(r, m)| (r.clone(), m.clone()))
            .collect())
    }

    /// Whether the GPU already holds the mesh, texture or environment `reference`.
    fn holds(&self, reference: &AssetRef) -> bool {
        self.assets.meshes.contains_key(reference)
            || self.assets.textures.contains_key(reference)
            || self.environments.holds(reference)
    }

    /// Instance and material blocks plus the draw order: opaque draws (culled pipelines first, then
    /// entity id), then blended draws back to front by the view depth of their bounds' center.
    fn instance_data(
        &self,
        world: &RenderWorld,
        materials: &BTreeMap<AssetRef, MaterialData>,
        view: &Mat4,
        frustum: &crate::cull::Frustum,
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
                blend: materials[&d.material].alpha_mode == AlphaMode::Blend,
                visible: {
                    let (lo, hi) = self.assets.meshes[&d.mesh].aabb;
                    frustum.sees(&d.world, lo, hi)
                },
            })
            .collect();
        let depth = |d: &DrawOrder| {
            let draw = &world.draws[d.item];
            let (lo, hi) = self.assets.meshes[&draw.mesh].aabb;
            view.transform_point3(draw.world.transform_point3((lo + hi) / 2.0))
                .z
        };
        order.sort_by(|a, b| match (a.blend, b.blend) {
            // Opaque: grouped by pipeline, material and mesh (so repeats instance), then entity.
            (false, false) => {
                let key = |d: &DrawOrder| {
                    (
                        d.double_sided,
                        d.material,
                        &world.draws[d.item].mesh,
                        d.item,
                    )
                };
                key(a).cmp(&key(b))
            }
            (true, true) => depth(a).total_cmp(&depth(b)).then(a.item.cmp(&b.item)),
            (x, y) => x.cmp(&y),
        });
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

    /// Builds the frame graph, places transients, records, then reads back or presents.
    fn execute(
        &mut self,
        view: &CameraView,
        plan: FramePlan<'_>,
        frame: &mut FrameResources,
    ) -> Result<(Option<Image>, Overflow), RenderError> {
        let FramePlan {
            draws,
            samples,
            size,
            cluster: job,
            shadow,
            sky,
            output,
            uploads,
        } = plan;
        let job = &job;
        let device = self.device.clone();
        let device = device.as_ref();
        let (w, h) = (size.0.get(), size.1.get());
        let clusters = job.grid.len() as u64;
        let overflow_rb = frame.keep(self.recycle.buffer(
            device,
            &BufferDesc {
                size: NonZeroU64::new(clusters * 4).unwrap_or(NonZeroU64::MIN),
                usage: BufferUsage::COPY_DST,
                memory: MemoryLocation::Readback,
                label: "cluster overflow".into(),
            },
        )?);
        let (texture, format, final_layout) = match output {
            Output::Readback => {
                let usage = TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC;
                (
                    frame.texture(device.create_texture(&desc(
                        size,
                        OUTPUT,
                        SampleCount::X1,
                        usage,
                        "output",
                    ))?),
                    OUTPUT,
                    Layout::CopySrc,
                )
            }
            Output::Surface {
                texture, format, ..
            } => (texture, format, Layout::Present),
        };
        let mut graph = RenderGraph::new();
        let out = graph.import_texture(
            "output",
            texture,
            desc(
                size,
                format,
                SampleCount::X1,
                TextureUsage::RENDER_TARGET,
                "output",
            ),
            Layout::Undefined,
            Some(final_layout),
        );
        let shadow_maps = shadow
            .map(|s| add_shadow_passes(&mut graph, s))
            .unwrap_or_default();
        let targets = add_forward(&mut graph, view, draws, samples, size, &shadow_maps, sky);
        let tonemap = self.pipelines.tonemap(device, format)?;
        add_tonemap(
            &mut graph,
            targets.hdr,
            out,
            tonemap,
            view.camera.tonemap,
            self.device.clone(),
        );
        let readback = match output {
            Output::Readback => Some(add_readback(
                &mut graph,
                out,
                frame,
                device,
                size,
                &self.recycle,
            )?),
            Output::Surface { .. } => None,
        };
        let compiled =
            graph.compile(&|d: &TextureDesc| device.texture_heap_footprint(d).unwrap_or((0, 1)))?;
        let transients = frame.place(device, &graph, &compiled, &mut self.recycle)?;
        let mut list = uploads;
        list.label = "frame".into();
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
        if let Output::Surface { surface, .. } = output {
            let value = self.frames + 1;
            let timeline = self.timeline(device)?;
            let on_presented = self.on_presented.take();
            device.present(
                surface,
                concordia_rhi::Present {
                    commands: &list,
                    signal: &[(timeline, value)],
                    on_presented,
                },
            )?;
            let pending = Overflow::Pending {
                buffer: overflow_rb,
                clusters,
                value,
            };
            return Ok((None, pending));
        }
        device.submit(Submit {
            queue: QueueKind::Graphics,
            commands: &list,
            wait: &[],
            signal: &[],
        })?;
        device.wait_idle()?;
        let image = match readback {
            Some(buffer) => Some(Image {
                width: w,
                height: h,
                rgba: device.read_buffer(buffer, 0, u64::from(w) * u64::from(h) * 4)?,
            }),
            None => None,
        };
        Ok((
            image,
            Overflow::Read(read_overflow(device, overflow_rb, clusters)?),
        ))
    }

    /// The presented-frame timeline (created on first use).
    fn timeline(&mut self, device: &dyn Device) -> Result<concordia_rhi::Timeline, RenderError> {
        match self.timeline {
            Some(t) => Ok(t),
            None => {
                let t = device.create_timeline(0)?;
                self.timeline = Some(t);
                Ok(t)
            }
        }
    }

    fn record_overflow(&mut self, overflow: u32) {
        self.stats.cluster_overflow = overflow;
        if overflow > 0 {
            tracing::warn!(target: "concordia::render", overflow, "clusters overflowed; raise max_per_cluster or reduce light ranges");
        }
    }

    /// Retires finished presented frames; with `wait`, first waits until at most `keep` remain.
    fn retire(&mut self, keep: usize) -> Result<(), RenderError> {
        let device = self.device.clone();
        if let (Some(timeline), Some(oldest)) = (
            self.timeline,
            self.in_flight
                .len()
                .checked_sub(keep + 1)
                .and_then(|i| self.in_flight.get(i)),
        ) {
            let ready = device.wait_timeline(timeline, oldest.value, FRAME_TIMEOUT)?;
            if !ready {
                return Err(RenderError::Gpu(concordia_rhi::RhiError::invalid(
                    "present",
                    "the GPU did not finish a frame within 10 s",
                )));
            }
        }
        let done = match self.timeline {
            Some(t) => device.timeline_value(t)?,
            None => return Ok(()),
        };
        while self.in_flight.front().is_some_and(|f| f.value <= done) {
            let Some(f) = self.in_flight.pop_front() else {
                break;
            };
            let overflow = read_overflow(device.as_ref(), f.overflow, f.clusters);
            f.resources.release(device.as_ref(), &mut self.recycle);
            self.record_overflow(overflow?);
        }
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = self.device.wait_idle();
        let _ = self.retire(0);
        self.recycle.clear(self.device.as_ref());
        self.assets.destroy(self.device.as_ref());
        self.shadow_maps.destroy(self.device.as_ref());
        self.environments.destroy(self.device.as_ref());
        self.surfaces
            .drain(..)
            .for_each(|s| self.device.destroy_surface(s));
    }
}

/// A camera at `view`, with the scene camera's settings (or defaults) and the view's projection.
fn editor_camera(world: &RenderWorld, view: &crate::View) -> Result<CameraView, RenderError> {
    let scene = world.camera(None).ok();
    let mut camera = scene.map(|c| c.camera.clone()).unwrap_or_default();
    camera.projection = view.projection;
    let entity = match scene {
        Some(c) => c.entity,
        None => EDITOR_VIEW
            .parse()
            .map_err(|e| RenderError::InvalidCamera(format!("editor view id: {e}")))?,
    };
    Ok(CameraView {
        entity,
        world: view.world_matrix()?,
        camera,
    })
}

/// Stands in for an entity id when an editor view renders a scene without cameras.
const EDITOR_VIEW: &str = "0193a1c2-7e40-7000-8000-00000000ed17";

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
    shadow_maps: &[GraphTexture],
    sky: Option<(Pipeline, Vec<u8>)>,
) -> ForwardTargets {
    let shadow_maps = shadow_maps.to_vec();
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
            for map in &shadow_maps {
                p.uses(*map, Use::SampledRead);
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
            if let Some((pipeline, push)) = &sky {
                ctx.commands.push(Command::SetPipeline(*pipeline));
                ctx.commands.push(Command::PushConstants(push.clone()));
                ctx.commands.push(Command::Draw {
                    vertices: 0..3,
                    instances: 0..1,
                });
            }
            let mut bound = None;
            for draw in &draws {
                if bound != Some(draw.pipeline) {
                    ctx.commands.push(Command::SetPipeline(draw.pipeline));
                    bound = Some(draw.pipeline);
                }
                ctx.commands.push(Command::PushConstants(draw.push.clone()));
                ctx.commands.push(Command::Draw {
                    vertices: 0..draw.vertices,
                    instances: 0..draw.instances,
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
    heap: Option<HeapEntry>,
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

    /// One heap sized by the graph's memory plan; each transient placed at its offset. Recycled
    /// heaps bring their placed textures, reused when the same texture lands at the same offset.
    fn place(
        &mut self,
        device: &dyn Device,
        graph: &RenderGraph,
        compiled: &crate::graph::CompiledGraph,
        recycle: &mut Recycler,
    ) -> Result<BTreeMap<GraphTexture, Texture>, RenderError> {
        let Some(size) = NonZeroU64::new(compiled.heap_size()) else {
            return Ok(BTreeMap::new());
        };
        let mut entry = match recycle
            .heaps
            .iter()
            .position(|h| h.size >= size && h.size.get() <= size.get() * 2)
        {
            Some(i) => recycle.heaps.swap_remove(i),
            None => {
                recycle.created += 1;
                HeapEntry {
                    heap: device.create_heap(&HeapDesc {
                        size,
                        label: "frame transients".into(),
                    })?,
                    size,
                    textures: Vec::new(),
                }
            }
        };
        let mut placed = BTreeMap::new();
        let mut used = Vec::new();
        for (t, placement) in compiled.placements() {
            let desc = graph.texture_desc(*t);
            let reuse = entry
                .textures
                .iter()
                .position(|(d, offset, _)| d == desc && *offset == placement.offset);
            let texture = match reuse {
                Some(i) => entry.textures.swap_remove(i),
                None => {
                    recycle.created += 1;
                    let texture =
                        device.create_placed_texture(entry.heap, placement.offset, desc)?;
                    (desc.clone(), placement.offset, texture)
                }
            };
            placed.insert(*t, texture.2);
            used.push(texture);
        }
        // Textures this frame did not need stay with the heap for later frames.
        entry.textures.extend(used);
        self.heap = Some(entry);
        Ok(placed)
    }

    /// Returns the frame's buffers and heap for reuse (the GPU must be done with them).
    fn release(self, device: &dyn Device, recycle: &mut Recycler) {
        self.textures
            .into_iter()
            .for_each(|t| device.destroy_texture(t));
        for buffer in self.buffers {
            let pooled = recycle
                .pool
                .lock()
                .is_ok_and(|mut p| p.give(device, buffer));
            if !pooled {
                device.destroy_buffer(buffer);
            }
        }
        if let Some(heap) = self.heap {
            recycle.heaps.push(heap);
            if recycle.heaps.len() > Renderer::MAX_FRAMES_IN_FLIGHT {
                let oldest = recycle.heaps.remove(0);
                oldest.destroy(device);
            }
        }
    }
}

/// A transient heap and the textures placed in it (kept across frames).
#[derive(Debug)]
struct HeapEntry {
    heap: concordia_rhi::Heap,
    size: NonZeroU64,
    textures: Vec<(TextureDesc, u64, Texture)>,
}

impl HeapEntry {
    fn destroy(self, device: &dyn Device) {
        self.textures
            .into_iter()
            .for_each(|(_, _, t)| device.destroy_texture(t));
        device.destroy_heap(self.heap);
    }
}

/// GPU memory retired frames hand back: pooled buffers and transient heaps.
#[derive(Debug, Default)]
struct Recycler {
    pool: SharedPool,
    heaps: Vec<HeapEntry>,
    /// Heaps and placed textures created so far.
    created: usize,
}

impl Recycler {
    /// Everything created so far (buffers, heaps, placed textures).
    fn created(&self) -> usize {
        self.created + self.pool.lock().map_or(0, |p| p.created)
    }

    /// A pooled buffer for `desc`.
    fn buffer(&self, device: &dyn Device, desc: &BufferDesc) -> Result<Buffer, RenderError> {
        let mut pool = self.pool.lock().map_err(|_| {
            RenderError::Gpu(concordia_rhi::RhiError::invalid("buffer pool", "poisoned"))
        })?;
        Ok(pool.take(device, desc)?)
    }

    fn clear(&mut self, device: &dyn Device) {
        self.heaps.drain(..).for_each(|h| h.destroy(device));
        if let Ok(mut pool) = self.pool.lock() {
            pool.clear(device);
        }
    }
}

/// How long a presented frame may take before the renderer reports the GPU stuck.
const FRAME_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Sums the per-cluster overflow counts copied into `buffer`.
fn read_overflow(device: &dyn Device, buffer: Buffer, clusters: u64) -> Result<u32, RenderError> {
    let dropped = device.read_buffer(buffer, 0, clusters * 4)?;
    Ok(dropped
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .sum())
}
