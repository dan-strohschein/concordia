//! The renderer: forward PBR into HDR, tonemapping into sRGB, readback, scheduled by the graph.

use std::collections::BTreeMap;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;

use concordia_assets::{AlphaMode, AssetRef, Material};
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Vec3};
use concordia_material::{ClosureBudget, ClosureReport};
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
use crate::frame::{DrawPush, FrameParams, IblParams, frame_block};
use crate::gpu::{GpuAssets, GpuMesh, SharedPool, Uploads};
use crate::graph::{GraphTexture, PassContext, RenderGraph, Use};
use crate::lighting::punctual_lights;
use crate::meshlet_pass::{BucketKey, FrameTables, MeshletJob, RoutePipelines};
use crate::pipelines::{DEPTH, Geometry, HDR, MOTION, OUTPUT, Pipelines};
use crate::programs::Programs;
use crate::shadow::{Cascade, CascadeSettings, fit_cascades};
use crate::shadow_pass::{
    ShadowDraw, ShadowJob, ShadowMaps, ShadowPush, add_shadow_passes, world_bounds,
};
use crate::skinning::{Placement, SkinFrame};
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
    /// Editor overlays to draw over the image (none for game views).
    pub overlays: Option<crate::Overlays>,
    /// The lit image or a debug view.
    pub view_mode: crate::ViewMode,
    /// Debug layers' world-space geometry, drawn over the image under the editor overlays.
    pub debug: Vec<concordia_api::DebugGeometry>,
    /// The in-game UI, drawn after tonemapping (ADR-0060).
    pub ui: concordia_api::UiDrawList,
}

/// A frame's motion vectors: per pixel, where its surface is now minus where it was last frame, in
/// UV units (the image is 1 × 1, y down), rows top to bottom (M4 4.3b2).
#[derive(Debug, Clone, PartialEq)]
pub struct MotionImage {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Motion per pixel.
    pub uv: Vec<[f32; 2]>,
}

impl MotionImage {
    /// The motion at pixel (`x`, `y`), in pixels.
    pub fn pixels(&self, x: u32, y: u32) -> [f32; 2] {
        let [u, v] = self.uv[(y * self.width + x) as usize];
        [u * self.width as f32, v * self.height as f32]
    }
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
    callbacks: FrameCallbacks,
    perf_mode: bool,
    /// Last frame's transforms, for motion vectors (4.3b2).
    motion: crate::motion::MotionHistory,
    /// Read each readback frame's motion vectors (`set_motion_readback`).
    motion_readback: bool,
    /// The last one read.
    last_motion: Option<MotionImage>,
    /// Draw static meshes with meshlets through the meshlet path (ADR-0100).
    meshlets: bool,
    /// A surface image acquired ahead of its frame (`acquire_ahead`), with its size.
    ahead: Option<(Surface, Texture, (u32, u32))>,
    environments: EnvironmentCache,
    shadow_maps: ShadowMaps,
    clusters: ClusterSettings,
    stats: RenderStats,
    assets: GpuAssets,
    /// The in-game UI's textures (images and glyph atlas pages).
    ui_textures: crate::ui_pass::UiTextures,
    frames: u64,
    /// Signalled with a frame's number when the GPU finishes a presented frame.
    timeline: Option<concordia_rhi::Timeline>,
    in_flight: std::collections::VecDeque<InFlight>,
    frames_in_flight: usize,
    /// Transient heaps of retired frames, reused by later ones.
    recycle: Recycler,
    /// What the shadow maps currently hold (see `shadow_key`).
    shadow_key: Option<u64>,
    /// Materials fitted to the closure budget, as programs.
    programs: Programs,
    /// GPU copies of forgotten assets; the next frame takes them and frees them when it retires,
    /// after every earlier frame that may still read them.
    retired: FrameResources,
    /// Skinned meshes already reported as drawn at their bind pose.
    unskinnable: std::collections::BTreeSet<AssetRef>,
    /// Entities whose invalid `Representation` was logged.
    invalid_representations: std::collections::BTreeSet<EntityId>,
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

/// What a presented frame reports back, and when.
#[derive(Default)]
pub struct FrameCallbacks {
    /// When the frame reached the display.
    pub presented: Option<concordia_rhi::PresentedCallback>,
    /// The frame's GPU time (ms).
    pub gpu_time: Option<concordia_rhi::GpuTimeCallback>,
}

impl std::fmt::Debug for FrameCallbacks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameCallbacks").finish_non_exhaustive()
    }
}

/// Where a draw item goes this frame.
struct DrawOrder {
    item: usize,
    material: u32,
    double_sided: bool,
    blend: bool,
    /// Inside the camera's view.
    visible: bool,
    /// Its mesh's mesh-table row, when it takes the meshlet path.
    meshlet_row: Option<u32>,
}

/// Per-frame instance, material and mesh-table bytes, and the draw order.
struct FrameBlocks {
    instances: Vec<u8>,
    materials: Vec<u8>,
    /// The meshlet path's mesh table (two `uint4` per mesh; see `meshlet::mesh_row`).
    meshes: Vec<u8>,
    order: Vec<DrawOrder>,
}

/// Everything `execute` records for one frame.
struct FramePlan<'a> {
    draws: Vec<Draw>,
    /// The meshlet path's work, when anything takes it.
    meshlets: Option<MeshletJob>,
    samples: SampleCount,
    size: (NonZeroU32, NonZeroU32),
    cluster: ClusterJob<'a>,
    shadow: Option<ShadowJob>,
    /// Skybox pipeline and push constants, drawn first in the forward pass.
    sky: Option<(Pipeline, Vec<u8>)>,
    output: Output,
    /// Upload copies (and their barrier), recorded first.
    uploads: CommandList,
    /// Editor overlay triangles (see `overlay::triangles`), drawn after tonemapping.
    overlay: Vec<f32>,
    /// The editor grid's pipeline and push constants, drawn after the scene in the forward pass.
    grid: Option<(Pipeline, Vec<u8>)>,
    /// The tonemapper (`Linear` for debug views, which show their data as is).
    tonemap: Tonemap,
    /// The in-game UI's buffers, drawn after tonemapping and before the editor overlays.
    ui: Option<UiPrepared>,
}

/// The UI's quads and stops uploaded for one frame.
#[derive(Debug, Clone, Copy)]
struct UiPrepared {
    quads: u32,
    stops: u32,
    count: u32,
}

/// How the tonemap pass draws the UI.
struct UiDraw {
    pipeline: Pipeline,
    push: Vec<u8>,
    vertices: u32,
}

/// The size and destination of one frame.
struct FrameTarget {
    size: (NonZeroU32, NonZeroU32),
    output: Output,
    overlays: Option<crate::Overlays>,
    view_mode: crate::ViewMode,
    debug: Vec<concordia_api::DebugGeometry>,
    ui: concordia_api::UiDrawList,
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

/// A side-effect pass `name` copying `out` (4 bytes a pixel) into a new readback buffer.
fn add_readback(
    graph: &mut RenderGraph,
    (name, out): (&'static str, GraphTexture),
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
        name,
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// Triangles the forward pass drew.
    pub triangles: u64,
    /// GPU memory the device has allocated, in bytes (if the backend can tell).
    pub memory_bytes: Option<u64>,
    /// GPU buffers, heaps and textures the frame had to create (0 once warm).
    pub allocations: usize,
    /// The sun's shadow maps were reused: no caster, light or cascade changed since they were drawn.
    pub shadows_cached: bool,
    /// The environment is still being prefiltered (this viewport frame was drawn without it).
    pub environment_pending: bool,
    /// Whether this frame's passes were timed (perf mode).
    pub pass_timing: crate::perf::PassTiming,
    /// The timed passes, in timer-scope order (scope `i` is `timed_passes[i]`).
    pub timed_passes: Vec<String>,
    /// How opaque static geometry was drawn (ADR-0100).
    pub raster_path: crate::RasterPath,
    /// What the meshlet path drew in the forward pass.
    pub meshlets: crate::MeshletCounts,
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
    /// Blended (drawn after the meshlet buckets).
    blend: bool,
}

impl Renderer {
    /// Compiles the shaders and creates the pipelines.
    pub fn new(
        device: Arc<dyn Device>,
        compiler: Box<dyn ShaderCompiler>,
    ) -> Result<Self, RenderError> {
        let culler = ClusterCuller::new(device.clone(), compiler.as_ref())?;
        let pipelines = Pipelines::new(device.as_ref(), compiler)?;
        let programs = Programs::new(crate::platform_closure_budget(device.caps()));
        Ok(Self {
            device,
            pipelines,
            culler,
            surfaces: Vec::new(),
            callbacks: FrameCallbacks::default(),
            perf_mode: false,
            motion: crate::motion::MotionHistory::default(),
            motion_readback: false,
            last_motion: None,
            meshlets: true,
            ahead: None,
            environments: EnvironmentCache::default(),
            shadow_maps: ShadowMaps::default(),
            clusters: ClusterSettings::default(),
            stats: RenderStats::default(),
            assets: GpuAssets::default(),
            ui_textures: crate::ui_pass::UiTextures::default(),
            frames: 0,
            timeline: None,
            in_flight: std::collections::VecDeque::new(),
            frames_in_flight: 2,
            recycle: Recycler::default(),
            shadow_key: None,
            programs,
            retired: FrameResources::default(),
            unskinnable: std::collections::BTreeSet::new(),
            invalid_representations: std::collections::BTreeSet::new(),
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

    /// Whether background work (environment prefiltering) will change later frames.
    pub fn has_pending_work(&self) -> bool {
        self.environments.pending()
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

    /// [`Renderer::render`], reporting the frame's GPU time through `callbacks.gpu_time`.
    pub fn render_with(
        &mut self,
        world: &RenderWorld,
        assets: &mut dyn AssetSource,
        request: &RenderRequest,
        callbacks: FrameCallbacks,
    ) -> Result<Image, RenderError> {
        self.callbacks = callbacks;
        let image = self.render(world, assets, request);
        self.callbacks = FrameCallbacks::default();
        image
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
        self.present_with(world, assets, request, surface, FrameCallbacks::default())
    }

    /// [`Renderer::present`], reporting the frame's display and GPU times through `callbacks`.
    pub fn present_with(
        &mut self,
        world: &RenderWorld,
        assets: &mut dyn AssetSource,
        request: &RenderRequest,
        surface: Surface,
        callbacks: FrameCallbacks,
    ) -> Result<bool, RenderError> {
        self.callbacks = callbacks;
        let (width, height) = check_size(request)?;
        let held = match self.ahead.take() {
            Some((s, texture, size)) if s == surface && size == (width.get(), height.get()) => {
                Some(texture)
            }
            Some((s, _, _)) => {
                self.hand_back(s);
                None
            }
            None => None,
        };
        let texture = match held {
            Some(texture) => texture,
            None => match self.device.acquire(surface, width.get(), height.get())? {
                Some(texture) => texture,
                None => return Ok(false),
            },
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
                self.hand_back(surface);
                Err(e)
            }
        }
    }

    /// Waits for and holds the surface's next image at `width`×`height`, so a following
    /// [`Renderer::present_with`] can use the newest input (late latching). `false` if the display
    /// has no image free.
    pub fn acquire_ahead(
        &mut self,
        surface: Surface,
        width: u32,
        height: u32,
    ) -> Result<bool, RenderError> {
        match &self.ahead {
            Some((s, _, size)) if *s == surface && *size == (width, height) => return Ok(true),
            Some((s, _, _)) => {
                let s = *s;
                self.ahead = None;
                self.hand_back(s);
            }
            None => {}
        }
        let texture = self.device.acquire(surface, width, height)?;
        self.ahead = texture.map(|t| (surface, t, (width, height)));
        Ok(self.ahead.is_some())
    }

    /// Returns an acquired image unused, so the surface is not left holding it.
    fn hand_back(&mut self, surface: Surface) {
        let _ = self.device.present(
            surface,
            concordia_rhi::Present::new(&CommandList::new("abandoned frame")),
        );
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
        // Representation LOD is this camera's choice for this frame only (ADR-0076, E2).
        self.report_invalid_representations(world);
        let world = &world.represented(crate::camera::eye(&view.world));
        let result = self.render_frame(
            world,
            assets,
            view,
            &grid,
            FrameTarget {
                size: (width, height),
                output,
                overlays: request.overlays.clone(),
                view_mode: request.view_mode,
                debug: request.debug.clone(),
                ui: request.ui.clone(),
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
        self.stats.clone()
    }

    /// The closure budget materials are fitted to.
    pub fn closure_budget(&self) -> ClosureBudget {
        self.programs.budget()
    }

    /// A pass timer for this frame when perf mode is on and the device can time passes; records
    /// which in the frame's stats.
    fn pass_timer(&mut self) -> Option<crate::perf::PassTimer> {
        use crate::perf::PassTiming;
        let can = self.device.caps().timestamp_granularity.times_passes();
        self.stats.pass_timing = match (self.perf_mode, can) {
            (false, _) => PassTiming::off,
            (true, false) => PassTiming::unsupported,
            (true, true) => PassTiming::measured,
        };
        (self.stats.pass_timing == PassTiming::measured).then(crate::perf::PassTimer::new)
    }

    /// The device this renderer draws with.
    pub fn device(&self) -> &Arc<dyn Device> {
        &self.device
    }

    /// Turns perf mode on or off: when on (and the device can), each pass is timed on its own.
    pub fn set_perf_mode(&mut self, on: bool) {
        self.perf_mode = on;
    }

    /// True if perf mode is on.
    pub fn perf_mode(&self) -> bool {
        self.perf_mode
    }

    /// Draws static meshes with meshlets through the meshlet path (on by default; ADR-0100).
    pub fn set_meshlets(&mut self, on: bool) {
        self.meshlets = on;
    }

    /// Reads each rendered frame's motion vectors back, for [`Renderer::read_motion`] (off by
    /// default: one more copy per frame).
    pub fn set_motion_readback(&mut self, on: bool) {
        self.motion_readback = on;
        if !on {
            self.last_motion = None;
        }
    }

    /// The last rendered frame's motion vectors, when readback is on.
    pub fn read_motion(&self) -> Option<&MotionImage> {
        self.last_motion.as_ref()
    }

    /// Forgets last frame's transforms, so the next frame has no motion: call it on camera cuts
    /// and scene loads (motion vectors, 4.3b2).
    pub fn reset_motion_history(&mut self) {
        self.motion.reset();
    }

    /// Whether the meshlet path is on.
    pub fn meshlets(&self) -> bool {
        self.meshlets
    }

    /// Sets the project's closure budget (None: the platform's); it can only lower the platform's.
    /// Returns the budget now in effect.
    pub fn set_closure_budget(&mut self, project: Option<ClosureBudget>) -> ClosureBudget {
        self.programs.set_project_budget(project)
    }

    /// Drops the GPU copy of a mesh, texture or environment map, so the next frame uploads it again.
    pub fn forget(&mut self, reference: &AssetRef) {
        if let Some(mesh) = self.assets.meshes.remove(reference) {
            self.retired.buffers.extend(mesh.buffers());
        }
        self.assets.joint_bounds.remove(reference);
        if let Some((texture, _)) = self.assets.textures.remove(reference) {
            self.retired.textures.push(texture);
        }
        self.retired
            .textures
            .extend(self.environments.forget(reference));
    }

    /// How many forward shaders specialized to a layered program shape have been compiled.
    pub fn material_variants(&self) -> usize {
        self.pipelines.material_variants()
    }

    /// The last frame's materials that were simplified to fit the closure budget.
    pub fn closure_reports(&self) -> &[(AssetRef, ClosureReport)] {
        self.programs.reports()
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
            overlays,
            view_mode,
            debug,
            ui,
        } = target;
        let aspect = width.get() as f32 / height.get() as f32;
        let view_m = view_matrix(&view.world);
        let view_proj = view.camera.projection.matrix(aspect)? * view_m;
        let samples = match view.camera.msaa {
            Msaa::Off => SampleCount::X1,
            Msaa::X4 => SampleCount::X4,
        };
        frame.adopt(std::mem::take(&mut self.retired));
        let mut up = Uploads::pooled(self.recycle.pool.clone());
        let mut resolved = crate::assets::resolve(world, assets, &|r| self.holds(r))?;
        let materials = self.load_assets(&resolved, &mut up)?;
        self.programs.fit(&materials)?;
        let skinning = SkinFrame::new(world, &self.assets.meshes, &self.assets.joint_bounds);
        self.report_unskinnable(&skinning.refused);
        let device = self.device.clone();
        let device = device.as_ref();
        let tables = self.environments.tables(device, &mut up)?;
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
        let shadows = self.prepare_shadows(world, &skinning.placements, view, aspect, dir)?;
        let palette_buf = frame.keep(up.buffer(device, &skinning.palette, "joint palettes")?);
        // Captures wait for a new environment's prefiltering; viewport frames draw without it.
        let wait = matches!(output, Output::Readback);
        let gpu = match &world.environment {
            Some(env) => {
                self.environments
                    .ensure(device, &mut up, &mut resolved, &env.map, wait)?
            }
            None => None,
        };
        self.stats.environment_pending = world.environment.is_some() && gpu.is_none();
        let environment = match (gpu, &world.environment) {
            (Some(gpu), Some(env)) => {
                let mut levels = [0u32; 6];
                gpu.levels
                    .iter()
                    .take(6)
                    .enumerate()
                    .for_each(|(i, (_, index))| levels[i] = *index);
                Some((gpu.sh, levels, env.skybox))
            }
            _ => None,
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
                index(palette_buf)?,
            ],
            shadow: shadow_params,
            ibl: environment.as_ref().map(|(sh, levels, skybox)| IblParams {
                sh,
                scale: world.environment.as_ref().map_or(0.0, |e| e.intensity) * exposure,
                skybox: *skybox,
                levels: *levels,
            }),
            lut: tables,
            view_mode: view_mode.shader_index(),
            exposure,
            prev_view_proj: self.motion.previous_view_proj(&view_proj),
        });
        let frame_buf = frame.keep(up.buffer(device, &block, "frame")?);
        let FrameBlocks {
            instances,
            materials: material_bytes,
            meshes: mesh_table,
            order,
        } = self.instance_data(
            world,
            &skinning.placements,
            &materials,
            &view_m,
            &crate::cull::Frustum::new(view_proj),
        )?;
        let inst_buf = frame.keep(up.buffer(device, &instances, "instances")?);
        let mat_buf = frame.keep(up.buffer(device, &material_bytes, "materials")?);
        let mesh_buf = frame.keep(up.buffer(device, &mesh_table, "meshlet meshes")?);
        let ui = self.ui_prepare(device, &mut up, frame, &ui)?;
        let (frame_i, inst_i, mat_i) = (index(frame_buf)?, index(inst_buf)?, index(mat_buf)?);
        let route = crate::meshlet::route(device.caps(), view_mode);
        let meshlet_geometry = match route {
            crate::meshlet::Route::ComputeIndirect => Geometry::Meshlets,
            crate::meshlet::Route::MeshTasks => Geometry::MeshTasks,
        };
        let mut path_draws: Vec<crate::meshlet::PathDraw<BucketKey>> = Vec::new();
        let mut meshlet_triangles = 0u64;
        let mut draws: Vec<Draw> = Vec::with_capacity(order.len());
        let mut casters: Vec<ShadowDraw> = Vec::new();
        let culled = order.iter().filter(|d| !d.visible).count();
        let mut casting = std::hash::DefaultHasher::new();
        for (
            instance,
            DrawOrder {
                item,
                material: material_index,
                double_sided,
                blend,
                visible,
                meshlet_row,
            },
        ) in order.into_iter().enumerate()
        {
            let instance = instance as u32;
            let mesh = self.assets.meshes[&world.draws[item].mesh];
            let placed = &skinning.placements[item];
            let casts = world.draws[item].cast_shadows;
            if casts {
                std::hash::Hash::hash(&mesh.indices, &mut casting);
                hash_matrix(&placed.world, &mut casting);
                if let Some(pose) = placed.pose {
                    world.skins[pose]
                        .palette
                        .iter()
                        .for_each(|m| hash_matrix(m, &mut casting));
                }
            }
            let shape = self.programs.shape(&world.draws[item].material).cloned();
            let variant = (samples, double_sided, blend);
            if let (Some(mesh_row), Some((_, info))) = (meshlet_row, mesh.meshlets) {
                if visible {
                    meshlet_triangles += u64::from(mesh.count / 3);
                }
                let forward = match visible {
                    true => Some((
                        self.pipelines.for_view(
                            device,
                            view_mode,
                            variant,
                            shape.as_ref(),
                            meshlet_geometry,
                        )?,
                        material_index,
                    )),
                    false => None,
                };
                path_draws.push(crate::meshlet::PathDraw {
                    instance,
                    mesh_row,
                    info,
                    forward,
                    casts,
                });
                continue;
            }
            if casts {
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
                            joints: mesh.joints,
                            weights: mesh.weights,
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
                joints: mesh.joints,
                weights: mesh.weights,
            };
            let pipeline = self.pipelines.for_view(
                device,
                view_mode,
                variant,
                shape.as_ref(),
                Geometry::Mesh,
            )?;
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
                    blend,
                }),
            }
        }
        let meshlet_plan = crate::meshlet::plan(&path_draws);
        let mut kept = Vec::new();
        let tables = FrameTables {
            frame: frame_i,
            instances: inst_i,
            materials: mat_i,
            meshes: index(mesh_buf)?,
        };
        let pipelines = match (route, self.pipelines.meshlet_shadow_mesh()) {
            (crate::meshlet::Route::MeshTasks, Some(shadow)) => {
                RoutePipelines::MeshTasks { shadow }
            }
            _ => RoutePipelines::ComputeIndirect {
                expand: self.pipelines.expand(),
                shadow: self.pipelines.meshlet_shadow(),
            },
        };
        let meshlets = MeshletJob::prepare(
            device,
            &mut up,
            &meshlet_plan,
            (tables, pipelines),
            &mut kept,
        )?;
        kept.into_iter().for_each(|b| {
            frame.keep(b);
        });
        let (uploads, staging) = up.into_commands();
        staging.into_iter().for_each(|b| {
            frame.keep(b);
        });
        self.stats.meshlets = meshlet_counts(&path_draws, &meshlet_plan, meshlet_triangles, route);
        self.stats.raster_path = match meshlets.as_ref().map(|m| m.route) {
            Some(crate::meshlet::Route::ComputeIndirect) => crate::RasterPath::compute_indirect,
            Some(crate::meshlet::Route::MeshTasks) => crate::RasterPath::mesh_shader,
            None => crate::RasterPath::forward,
        };
        self.stats.draws = world.draws.len() - culled;
        self.stats.culled = culled;
        self.stats.draw_calls = draws.len()
            + (self.stats.meshlets.indirect_draws + self.stats.meshlets.mesh_draws) as usize;
        self.stats.triangles = draws
            .iter()
            .map(|d| u64::from(d.vertices / 3) * u64::from(d.instances))
            .sum::<u64>()
            + meshlet_triangles;
        self.stats.memory_bytes = device.allocated_bytes();
        let job = ClusterJob {
            buffers: &cluster,
            grid,
            light_count: lights.bounds.len(),
        };
        let sky_shown = view_mode.draws_sky();
        let sky = match sky_shown && environment.as_ref().is_some_and(|(_, _, skybox)| *skybox) {
            true => Some((
                self.pipelines.sky(device, samples)?,
                frame_i.to_le_bytes().to_vec(),
            )),
            false => None,
        };
        let shadow = shadows.map(|(cascades, settings, maps)| {
            let key = shadow_key(
                std::hash::Hasher::finish(&casting),
                &cascades,
                settings.resolution,
                &maps,
            );
            let reuse = self.shadow_key == Some(key);
            self.shadow_key = Some(key);
            self.stats.shadows_cached = reuse;
            ShadowJob {
                maps,
                resolution: settings.resolution,
                pipeline: self.pipelines.shadow(),
                casters,
                meshlets: meshlets.as_ref().and_then(|m| m.shadow.clone()),
                reuse,
            }
        });
        if shadow.is_none() {
            self.shadow_key = None;
            self.stats.shadows_cached = false;
        }
        self.motion.advance(
            world
                .draws
                .iter()
                .zip(&skinning.placements)
                .map(|(d, placed)| (d.entity, placed.world)),
            view_proj,
        );
        let plan = FramePlan {
            draws,
            meshlets,
            samples,
            size: (width, height),
            cluster: job,
            shadow,
            sky,
            output,
            uploads,
            grid: match overlays.as_ref().is_some_and(|o| o.grid) {
                true => {
                    let height = eye(&view.world).y.abs();
                    let extent = (height * 40.0).clamp(50.0, 2000.0);
                    let mut push = frame_i.to_le_bytes().to_vec();
                    push.extend(extent.to_le_bytes());
                    Some((self.pipelines.grid(device, samples)?, push))
                }
                false => None,
            },
            overlay: overlays
                .clone()
                .or_else(|| (!debug.is_empty()).then(crate::Overlays::default))
                .as_ref()
                .map_or_else(Vec::new, |o| {
                    let projector = crate::overlay::Projector::new(
                        view_proj,
                        width.get() as f32,
                        height.get() as f32,
                    );
                    let boxes: Vec<_> = skinning
                        .placements
                        .iter()
                        .map(|p| (p.world, p.bounds))
                        .collect();
                    crate::overlay::triangles(o, world, &projector, &boxes, &debug)
                }),
            tonemap: match view_mode.tonemapped() {
                true => view.camera.tonemap,
                false => Tonemap::Linear,
            },
            ui,
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
        placements: &[Placement],
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
            .zip(placements)
            .filter(|(d, _)| d.cast_shadows)
            .map(|(_, p)| (p.bounds, p.world));
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

    /// Uploads the meshes and textures of `resolved` (what the GPU lacks) and returns its materials.
    fn load_assets(
        &mut self,
        resolved: &MemoryAssets,
        up: &mut Uploads,
    ) -> Result<BTreeMap<AssetRef, Material>, RenderError> {
        let device = self.device.as_ref();
        for (reference, data) in resolved.meshes() {
            let mesh = GpuMesh::upload(device, up, data, &reference.to_string())?;
            if mesh.joint_count > 0 {
                let bounds = concordia_anim::joint_bounds(data);
                self.assets.joint_bounds.insert(reference.clone(), bounds);
            }
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

    /// Logs, once per mesh, skinned meshes drawn at their bind pose.
    /// Logs each entity whose `Representation` is invalid once (it is drawn full).
    fn report_invalid_representations(&mut self, world: &RenderWorld) {
        for draw in &world.draws {
            let Some((by, representation)) = &draw.representation else {
                continue;
            };
            if let Err(e) = representation.validate()
                && self.invalid_representations.insert(*by)
            {
                tracing::warn!(target: "concordia::render", entity = %by, error = %e, "Representation ignored; drawn in full");
            }
        }
    }

    fn report_unskinnable(&mut self, refused: &[AssetRef]) {
        for mesh in refused {
            if self.unskinnable.insert(mesh.clone()) {
                tracing::warn!(target: "concordia::render", %mesh, "skinned mesh drawn at its bind pose: it lacks joint streams, or uses joints its palette lacks");
            }
        }
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
        placements: &[Placement],
        materials: &BTreeMap<AssetRef, Material>,
        view: &Mat4,
        frustum: &crate::cull::Frustum,
    ) -> Result<FrameBlocks, RenderError> {
        let texture = |r: &AssetRef| self.assets.textures.get(r).map(|(_, i)| *i);
        let (material_bytes, indices) = self.programs.buffer(materials.keys(), &texture)?;
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
                    let (lo, hi) = placements[item].bounds;
                    frustum.sees(&placements[item].world, lo, hi)
                },
                meshlet_row: None,
            })
            .collect();
        let depth = |d: &DrawOrder| {
            let placed = &placements[d.item];
            let (lo, hi) = placed.bounds;
            view.transform_point3(placed.world.transform_point3((lo + hi) / 2.0))
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
        let meshes = self.assign_mesh_rows(world, placements, &mut order);
        let mut instances = Vec::new();
        order.iter().for_each(|d| {
            let placed = &placements[d.item];
            let draw = &world.draws[d.item];
            let mut skin = placed.skin;
            skin[2] = lod_code(draw.sim_lod, draw.instanced);
            skin[3] = d.meshlet_row.map_or(0, |row| row + 1);
            crate::frame::InstanceRow {
                model: placed.world,
                previous: self.motion.previous_world(draw.entity, &placed.world),
                skin,
                motion: [0; 4],
            }
            .write(&mut instances);
        });
        debug_assert_eq!(instances.len(), order.len() * crate::frame::INSTANCE_BYTES);
        Ok(FrameBlocks {
            instances,
            materials: material_bytes,
            meshes,
            order,
        })
    }

    /// Gives each draw taking the meshlet path its mesh's row, and returns the mesh table.
    fn assign_mesh_rows(
        &self,
        world: &RenderWorld,
        placements: &[Placement],
        order: &mut [DrawOrder],
    ) -> Vec<u8> {
        let mut rows: BTreeMap<&AssetRef, u32> = BTreeMap::new();
        let mut table = Vec::new();
        for d in order.iter_mut() {
            let reference = &world.draws[d.item].mesh;
            let mesh = &self.assets.meshes[reference];
            let skinned = placements[d.item].pose.is_some() || mesh.joint_count > 0;
            let info = mesh.meshlets.map(|(_, info)| info);
            if !crate::meshlet::takes_path(self.meshlets, info, skinned, d.blend) {
                continue;
            }
            let next = rows.len() as u32;
            let row = *rows.entry(reference).or_insert_with(|| {
                let (blob, info) = mesh.meshlets.unwrap_or_default();
                let streams = crate::meshlet::Streams {
                    positions: mesh.positions,
                    normals: mesh.normals,
                    tangents: mesh.tangents,
                    uvs: mesh.uvs,
                };
                crate::frame::push_rows_u32(
                    &mut table,
                    &crate::meshlet::mesh_row(blob, streams, info),
                );
                next
            });
            d.meshlet_row = Some(row);
        }
        table
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
            meshlets,
            samples,
            size,
            cluster: job,
            shadow,
            sky,
            output,
            uploads,
            overlay,
            grid,
            tonemap: tonemap_mode,
            ui,
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
        if let Some(m) = &meshlets {
            m.add_pass(&mut graph);
        }
        let shadow_maps = shadow
            .map(|s| add_shadow_passes(&mut graph, s))
            .unwrap_or_default();
        let targets = add_forward(
            &mut graph,
            view,
            (draws, meshlets),
            samples,
            size,
            &shadow_maps,
            (sky, grid),
        );
        let tonemap = self.pipelines.tonemap(device, format)?;
        let overlay = match overlay.is_empty() {
            true => None,
            false => Some(self.overlay_draw(device, frame, &overlay, format, size)?),
        };
        let ui = match ui {
            None => None,
            Some(u) => Some(UiDraw {
                pipeline: self.pipelines.ui(device, format)?,
                push: crate::ui_pass::UiPush {
                    quads: u.quads,
                    stops: u.stops,
                    width: w as f32,
                    height: h as f32,
                }
                .bytes(),
                vertices: u.count * 6,
            }),
        };
        add_tonemap(
            &mut graph,
            targets.hdr,
            out,
            (tonemap, ui, overlay),
            tonemap_mode,
            self.device.clone(),
        );
        let readback = match output {
            Output::Readback => Some(add_readback(
                &mut graph,
                ("readback", out),
                frame,
                device,
                size,
                &self.recycle,
            )?),
            Output::Surface { .. } => None,
        };
        let motion_readback = match (output, self.motion_readback) {
            (Output::Readback, true) => Some(add_readback(
                &mut graph,
                ("motion readback", targets.motion),
                frame,
                device,
                size,
                &self.recycle,
            )?),
            _ => None,
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
        let mut timer = self.pass_timer();
        if let Some(t) = &mut timer {
            t.begin(&mut list, "lights.cluster");
        }
        self.culler.record(
            &mut list,
            job.buffers,
            job.grid,
            job.light_count,
            fragment_read,
        );
        match &mut timer {
            Some(t) => {
                t.end(&mut list);
                compiled.execute_timed(&graph, &transients, &mut list, t)?;
            }
            None => compiled.execute(&graph, &transients, &mut list)?,
        }
        self.stats.timed_passes = timer.map(|t| t.names().to_vec()).unwrap_or_default();
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
            let callbacks = std::mem::take(&mut self.callbacks);
            device.present(
                surface,
                concordia_rhi::Present {
                    commands: &list,
                    signal: &[(timeline, value)],
                    on_presented: callbacks.presented,
                    on_gpu_time: callbacks.gpu_time,
                },
            )?;
            let pending = Overflow::Pending {
                buffer: overflow_rb,
                clusters,
                value,
            };
            return Ok((None, pending));
        }
        let submit = Submit {
            queue: QueueKind::Graphics,
            commands: &list,
            wait: &[],
            signal: &[],
        };
        match std::mem::take(&mut self.callbacks).gpu_time {
            Some(on_gpu_time) => device.submit_timed(submit, on_gpu_time)?,
            None => device.submit(submit)?,
        }
        device.wait_idle()?;
        let image = match readback {
            Some(buffer) => Some(Image {
                width: w,
                height: h,
                rgba: device.read_buffer(buffer, 0, u64::from(w) * u64::from(h) * 4)?,
            }),
            None => None,
        };
        if let Some(buffer) = motion_readback {
            let bytes = device.read_buffer(buffer, 0, u64::from(w) * u64::from(h) * 4)?;
            self.last_motion = Some(MotionImage {
                width: w,
                height: h,
                uv: crate::motion::decode_rg16f(&bytes),
            });
        }
        Ok((
            image,
            Overflow::Read(read_overflow(device, overflow_rb, clusters)?),
        ))
    }

    /// Uploads the UI's textures (cached by key and generation) and this frame's quads and stops.
    fn ui_prepare(
        &mut self,
        device: &dyn Device,
        up: &mut Uploads,
        frame: &mut FrameResources,
        list: &concordia_api::UiDrawList,
    ) -> Result<Option<UiPrepared>, RenderError> {
        if list.is_empty() {
            return Ok(None);
        }
        let textures = self
            .ui_textures
            .prepare(device, up, list, &mut frame.textures)?;
        let (quads, mut stops) = crate::ui_pass::encode_ui(list, &textures);
        if stops.is_empty() {
            stops = vec![0.0; crate::ui_pass::UI_STOP_FLOATS];
        }
        let upload =
            |renderer: &mut Self, frame: &mut FrameResources, floats: &[f32], label: &str| {
                let bytes: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
                let buffer = frame.keep(renderer.recycle.buffer(
                    device,
                    &BufferDesc {
                        size: NonZeroU64::new(bytes.len() as u64).unwrap_or(NonZeroU64::MIN),
                        usage: BufferUsage::STORAGE | BufferUsage::BINDLESS,
                        memory: MemoryLocation::Upload,
                        label: label.into(),
                    },
                )?);
                device.write_buffer(buffer, 0, &bytes)?;
                Ok::<u32, RenderError>(device.buffer_index(buffer)?.0)
            };
        let quads_i = upload(self, frame, &quads, "ui quads")?;
        let stops_i = upload(self, frame, &stops, "ui stops")?;
        Ok(Some(UiPrepared {
            quads: quads_i,
            stops: stops_i,
            count: list.quads.len() as u32,
        }))
    }

    /// Uploads overlay triangles for this frame and returns how to draw them.
    fn overlay_draw(
        &mut self,
        device: &dyn Device,
        frame: &mut FrameResources,
        triangles: &[f32],
        format: concordia_rhi::Format,
        size: (NonZeroU32, NonZeroU32),
    ) -> Result<OverlayDraw, RenderError> {
        let bytes: Vec<u8> = triangles.iter().flat_map(|f| f.to_le_bytes()).collect();
        let buffer = frame.keep(self.recycle.buffer(
            device,
            &BufferDesc {
                size: NonZeroU64::new(bytes.len() as u64).unwrap_or(NonZeroU64::MIN),
                usage: BufferUsage::STORAGE | BufferUsage::BINDLESS,
                memory: MemoryLocation::Upload,
                label: "editor overlays".into(),
            },
        )?);
        device.write_buffer(buffer, 0, &bytes)?;
        Ok(OverlayDraw {
            pipeline: self.pipelines.overlay(device, format)?,
            buffer: device.buffer_index(buffer)?.0,
            vertices: (triangles.len() / crate::overlay::VERTEX_FLOATS) as u32,
            size: [size.0.get() as f32, size.1.get() as f32],
        })
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
        std::mem::take(&mut self.retired).release(self.device.as_ref(), &mut self.recycle);
        self.recycle.clear(self.device.as_ref());
        self.assets.destroy(self.device.as_ref());
        self.ui_textures.destroy(self.device.as_ref());
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
pub(crate) const EDITOR_VIEW: &str = "0193a1c2-7e40-7000-8000-00000000ed17";

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
    /// Motion vectors, resolved (M4 4.3b2).
    motion: GraphTexture,
}

/// Forward pass: draws into (multisampled) HDR + depth, resolving into single-sample HDR.
/// A pipeline and its push constants, for a one-call draw (the sky, the grid).
type FullDraw = Option<(Pipeline, Vec<u8>)>;

fn add_forward(
    graph: &mut RenderGraph,
    view: &CameraView,
    (draws, meshlets): (Vec<Draw>, Option<MeshletJob>),
    samples: SampleCount,
    size: (NonZeroU32, NonZeroU32),
    shadow_maps: &[GraphTexture],
    (sky, grid): (FullDraw, FullDraw),
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
    let motion = graph.create_texture(desc(
        size,
        MOTION,
        SampleCount::X1,
        TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED | TextureUsage::COPY_SRC,
        "motion",
    ));
    let motion_msaa = (samples != SampleCount::X1).then(|| {
        graph.create_texture(desc(
            size,
            MOTION,
            samples,
            TextureUsage::RENDER_TARGET,
            "motion_msaa",
        ))
    });
    let clear = view.camera.clear;
    graph.add_pass_recorded(
        "forward",
        |p| {
            p.uses(depth, Use::DepthWrite);
            p.uses(hdr, Use::ColorWrite);
            p.uses(motion, Use::ColorWrite);
            for m in msaa.iter().chain(&motion_msaa) {
                p.uses(*m, Use::ColorWrite);
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
            let (motion_target, motion_resolve) = match motion_msaa {
                Some(m) => (ctx.texture(m), Some(ctx.texture(motion))),
                None => (ctx.texture(motion), None),
            };
            ctx.commands.push(Command::BeginRenderPass(RenderPassDesc {
                colors: vec![
                    ColorAttachment {
                        texture: target,
                        resolve,
                        load: LoadOp::Clear(ClearValue::Color(clear)),
                        store,
                    },
                    ColorAttachment {
                        texture: motion_target,
                        resolve: motion_resolve,
                        load: LoadOp::Clear(ClearValue::Color([0.0; 4])),
                        store,
                    },
                ],
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
            let opaque = draws.iter().take_while(|d| !d.blend).count();
            for (i, draw) in draws.iter().enumerate() {
                if i == opaque {
                    bound = draw_buckets(meshlets.as_ref(), &mut ctx.commands.commands, bound);
                }
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
            if opaque == draws.len() {
                draw_buckets(meshlets.as_ref(), &mut ctx.commands.commands, bound);
            }
            if let Some((pipeline, push)) = &grid {
                ctx.commands.push(Command::SetPipeline(*pipeline));
                ctx.commands.push(Command::PushConstants(push.clone()));
                ctx.commands.push(Command::Draw {
                    vertices: 0..6,
                    instances: 0..1,
                });
            }
            ctx.commands.push(Command::EndRenderPass);
            Ok(())
        })),
    );
    ForwardTargets { hdr, motion }
}

/// What the meshlet path draws in the forward pass (`triangles`: its instances' source triangles).
fn meshlet_counts(
    draws: &[crate::meshlet::PathDraw<BucketKey>],
    plan: &crate::meshlet::MeshletPlan<BucketKey>,
    triangles: u64,
    route: crate::meshlet::Route,
) -> crate::MeshletCounts {
    let forward = plan.buckets.iter().filter(|b| b.key.is_some());
    let buckets = forward.clone().count() as u64;
    let mesh = route == crate::meshlet::Route::MeshTasks;
    crate::MeshletCounts {
        instances: draws.iter().filter(|d| d.forward.is_some()).count() as u64,
        meshlets: forward.map(|b| u64::from(b.entries)).sum(),
        triangles,
        indirect_draws: if mesh { 0 } else { buckets },
        mesh_draws: if mesh { buckets } else { 0 },
    }
}

/// Records the meshlet buckets' draws; returns the pipeline left bound.
fn draw_buckets(
    meshlets: Option<&MeshletJob>,
    commands: &mut Vec<Command>,
    mut bound: Option<Pipeline>,
) -> Option<Pipeline> {
    let Some(job) = meshlets else {
        return bound;
    };
    for bucket in &job.forward {
        if bound != Some(bucket.pipeline) {
            commands.push(Command::SetPipeline(bucket.pipeline));
            bound = Some(bucket.pipeline);
        }
        commands.extend(bucket.commands(None));
    }
    bound
}

/// Tonemap pass: HDR (read by bindless index) → sRGB output.
/// Editor overlay triangles uploaded for one frame.
struct OverlayDraw {
    pipeline: Pipeline,
    buffer: u32,
    vertices: u32,
    size: [f32; 2],
}

fn add_tonemap(
    graph: &mut RenderGraph,
    hdr: GraphTexture,
    out: GraphTexture,
    (pipeline, ui, overlay): (Pipeline, Option<UiDraw>, Option<OverlayDraw>),
    tonemap: Tonemap,
    device: Arc<dyn Device>,
) {
    let mode = crate::components::tonemap_shader_mode(tonemap);
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
            // The in-game UI in the same pass, in display space, under the editor overlays.
            if let Some(u) = &ui {
                ctx.commands.push(Command::SetPipeline(u.pipeline));
                ctx.commands.push(Command::PushConstants(u.push.clone()));
                ctx.commands.push(Command::Draw {
                    vertices: 0..u.vertices,
                    instances: 0..1,
                });
            }
            // Editor overlays in the same pass, over the tonemapped image.
            if let Some(o) = &overlay {
                ctx.commands.push(Command::SetPipeline(o.pipeline));
                let mut push = o.buffer.to_le_bytes().to_vec();
                push.extend(o.size[0].to_le_bytes());
                push.extend(o.size[1].to_le_bytes());
                ctx.commands.push(Command::PushConstants(push));
                ctx.commands.push(Command::Draw {
                    vertices: 0..o.vertices,
                    instances: 0..1,
                });
            }
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
    /// Takes over `other`'s buffers and textures (freed with this frame's).
    fn adopt(&mut self, other: FrameResources) {
        debug_assert!(other.heap.is_none(), "only frames own heaps");
        self.buffers.extend(other.buffers);
        self.textures.extend(other.textures);
    }

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

/// The `lod` view mode's per-instance code: simulation level + 1 (0: none), plus 16 if instanced.
fn lod_code(sim_lod: Option<u8>, instanced: bool) -> u32 {
    let level = sim_lod.map_or(0, |l| u32::from(l.min(3)) + 1);
    level | if instanced { 16 } else { 0 }
}

fn hash_matrix(m: &Mat4, hasher: &mut impl std::hash::Hasher) {
    m.to_cols_array()
        .iter()
        .for_each(|v| hasher.write_u32(v.to_bits()));
}

/// Identifies the shadow maps' contents: the casters (meshes and world matrices), the cascades
/// and the maps themselves.
fn shadow_key(casters: u64, cascades: &[Cascade], resolution: u32, maps: &[(Texture, u32)]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    casters.hash(&mut h);
    resolution.hash(&mut h);
    maps.hash(&mut h);
    cascades
        .iter()
        .for_each(|c| hash_matrix(&c.view_proj, &mut h));
    h.finish()
}
