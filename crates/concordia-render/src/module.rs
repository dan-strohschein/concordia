//! The render module: render components and `capture.screenshot` (headless, offscreen).

use std::sync::Arc;

use concordia_api::{
    ApiError, ApiErrorKind, CallCtx, EncodedImage, Engine, EngineModule, EntityRef, Method,
    MethodInfo, WorldSel,
};
use concordia_assets::AssetDb;
use concordia_command::Actor;
use concordia_reflect::Reflect;
use concordia_rhi::{Device, RhiError};
use concordia_shaders::{SlangToolchain, SlangcCompiler};
use concordia_terra::SimClock;
use serde::{Deserialize, Serialize};

use std::collections::BTreeSet;

use concordia_assets::AssetRef;

pub use crate::frame_log::FrameRecord;
use crate::frame_log::{FrameLog, push_frame, remove_frame, unix_ms};
use crate::thread::{RenderThread, ViewportFrame};
use crate::{Image, MemoryAssets, RenderError, RenderRequest, RenderWorld, Renderer};

/// Creates the GPU device on first use (so engines start without one).
pub type DeviceFactory = Arc<dyn Fn() -> Result<Box<dyn Device>, RhiError> + Send + Sync>;

/// Registers render components and `capture.screenshot`.
pub struct RenderModule {
    factory: DeviceFactory,
}

impl std::fmt::Debug for RenderModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderModule").finish_non_exhaustive()
    }
}

impl RenderModule {
    /// A module that renders on devices from `factory`.
    pub fn new(factory: DeviceFactory) -> Self {
        Self { factory }
    }
}

/// Engine-lifetime state: the device factory and, once something renders, the render thread.
struct RenderState {
    factory: DeviceFactory,
    thread: Option<RenderThread>,
    /// Whether a host attached the editor's native viewport.
    surface: bool,
    /// Meshes and textures already sent to the render thread (it keeps them).
    shipped: BTreeSet<AssetRef>,
    /// Recent native-viewport frames (for `viewport.stats`), keyed by frame number.
    frames: FrameLog,
    next_frame: u64,
}

impl EngineModule for RenderModule {
    fn name(&self) -> &'static str {
        "render"
    }

    fn register_types(
        &self,
        registry: &mut concordia_reflect::TypeRegistry,
    ) -> Result<(), concordia_reflect::RegistryError> {
        crate::register_types(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![
            MethodInfo::of::<CaptureScreenshot>(),
            MethodInfo::of::<ViewportPick>(),
            MethodInfo::of::<ViewportPresent>(),
            MethodInfo::of::<ViewportStats>(),
        ]
    }

    fn engine_state(&self) -> Option<Box<dyn std::any::Any + Send>> {
        Some(Box::new(RenderState {
            factory: self.factory.clone(),
            thread: None,
            surface: false,
            shipped: BTreeSet::new(),
            frames: FrameLog::default(),
            next_frame: 0,
        }))
    }
}

/// Render a camera's view offscreen at any size (independent of any window) and return a PNG.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct CaptureScreenshot;

/// `capture.screenshot` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct CaptureScreenshotParams {
    /// Camera entity; default: the highest-priority Camera.
    #[serde(default)]
    pub camera: Option<EntityRef>,
    /// Render from this viewpoint instead of a camera entity (the editor's view).
    #[serde(default)]
    pub view: Option<crate::View>,
    /// Width in pixels.
    #[serde(default = "default_width")]
    #[reflect(default = 1280, range(min = 1.0, max = 8192.0))]
    pub width: u32,
    /// Height in pixels.
    #[serde(default = "default_height")]
    #[reflect(default = 720, range(min = 1.0, max = 8192.0))]
    pub height: u32,
    /// Which world to render: edit (default) or play.
    #[serde(default)]
    pub world: Option<WorldSel>,
    /// Who is asking (attribution in the log only).
    #[serde(default)]
    pub actor: Option<Actor>,
}

fn default_width() -> u32 {
    1280
}

fn default_height() -> u32 {
    720
}

/// `capture.screenshot` result (MCP returns `image` as image content).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct CaptureScreenshotResult {
    /// The PNG.
    pub image: EncodedImage,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The rendered world's simulation tick (0 for the edit world).
    pub sim_tick: u64,
    /// Captures rendered by this engine so far (this one included).
    pub frame: u64,
}

impl Method for CaptureScreenshot {
    const NAME: &'static str = "capture.screenshot";
    const MUTATES: bool = false;
    type Params = CaptureScreenshotParams;
    type Result = CaptureScreenshotResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: CaptureScreenshotParams,
    ) -> Result<CaptureScreenshotResult, ApiError> {
        if p.camera.is_some() && p.view.is_some() {
            return Err(ApiError::new(
                ApiErrorKind::InvalidParams,
                "pass either camera or view, not both",
            )
            .at("/view"));
        }
        p.view.as_ref().map(check_view).transpose()?;
        let world = engine.world(p.world.unwrap_or_default())?;
        let camera = p.camera.as_ref().map(|c| c.resolve(world)).transpose()?;
        let sim_tick = world.resource::<SimClock>().map_or(0, |c| c.tick.0);
        let snapshot = RenderWorld::extract(world);
        let request = RenderRequest {
            camera,
            view: p.view.clone(),
            width: p.width,
            height: p.height,
        };
        let assets = ship(engine, &snapshot)?;
        let (image, frame) = render_thread(engine)?
            .capture(snapshot, assets, request)
            .map_err(api_error)?;
        let actor = ctx
            .actor
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        tracing::debug!(target: "concordia::render", actor, width = p.width, height = p.height, sim_tick, frame, "captured screenshot");
        Ok(CaptureScreenshotResult {
            image: EncodedImage::png(&encode_png(&image)?),
            width: image.width,
            height: image.height,
            sim_tick,
            frame,
        })
    }
}

/// The render thread, started on first use (with its device and renderer).
fn render_thread(engine: &mut Engine) -> Result<&RenderThread, ApiError> {
    let state = engine
        .state_mut::<RenderState>()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the render module has no state"))?;
    if state.thread.is_none() {
        let factory = state.factory.clone();
        state.thread =
            Some(RenderThread::spawn(move || make_renderer(&factory)).map_err(api_error)?);
    }
    state
        .thread
        .as_ref()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the render thread did not start"))
}

fn make_renderer(factory: &DeviceFactory) -> Result<Renderer, RenderError> {
    let device = factory().map_err(RenderError::Gpu)?;
    let toolchain = SlangToolchain::locate().map_err(|e| RenderError::Shader(e.to_string()))?;
    Renderer::new(Arc::from(device), Box::new(SlangcCompiler::new(toolchain)))
}

/// The asset data `world` needs that the render thread has not been sent yet.
fn ship(engine: &mut Engine, world: &RenderWorld) -> Result<MemoryAssets, ApiError> {
    let shipped = engine
        .state_mut::<RenderState>()
        .map(|s| std::mem::take(&mut s.shipped))
        .unwrap_or_default();
    let held = |r: &AssetRef| shipped.contains(r);
    let resolved = match engine.state_mut::<AssetDb>() {
        Some(db) => crate::resolve(world, db, &held),
        None => crate::resolve(world, &mut MemoryAssets::default(), &held),
    };
    let mut shipped = shipped;
    if let Ok(assets) = &resolved {
        shipped.extend(assets.cached_kinds().cloned());
    }
    if let Some(state) = engine.state_mut::<RenderState>() {
        state.shipped = shipped;
    }
    resolved.map_err(api_error)
}

/// Deterministic PNG bytes (RGBA8, default compression).
fn encode_png(image: &Image) -> Result<Vec<u8>, ApiError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let fail = |e: png::EncodingError| {
        ApiError::new(ApiErrorKind::Internal, format!("PNG encoding failed: {e}"))
    };
    let mut writer = encoder.write_header().map_err(fail)?;
    writer.write_image_data(&image.rgba).map_err(fail)?;
    writer.finish().map_err(fail)?;
    Ok(out)
}

/// Maps renderer errors to API errors agents can act on.
fn api_error(error: RenderError) -> ApiError {
    let message = error.to_string();
    match error {
        RenderError::NoCamera => ApiError::new(ApiErrorKind::InvalidParams, message)
            .suggest(["spawn an entity with Transform and Camera components".to_owned()]),
        RenderError::UnknownCamera(_) => ApiError::new(ApiErrorKind::InvalidParams, message)
            .suggest(["query.run with {\"with\": [\"Camera\"]} lists cameras".to_owned()])
            .at("/camera"),
        RenderError::InvalidSize { .. } => {
            ApiError::new(ApiErrorKind::InvalidParams, message).at("/width")
        }
        RenderError::InvalidCamera(_) | RenderError::Asset { .. } => {
            ApiError::new(ApiErrorKind::ValueInvalid, message)
        }
        RenderError::Gpu(RhiError::NoDevice { .. }) => {
            ApiError::new(ApiErrorKind::GpuUnavailable, message).suggest([
                "run `concordia gpu-probe`; rendering needs Metal (macOS) in M1".to_owned(),
            ])
        }
        _ => ApiError::new(ApiErrorKind::Internal, message),
    }
}

/// The entity whose mesh is under pixel (x, y) of a width×height image rendered from `view` (nearest first; bounds-based).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportPick;

/// `viewport.pick` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ViewportPickParams {
    /// The viewpoint.
    pub view: crate::View,
    /// Pixel column (0 = left).
    pub x: u32,
    /// Pixel row (0 = top).
    pub y: u32,
    /// Image width in pixels.
    #[reflect(range(min = 1.0, max = 8192.0))]
    pub width: u32,
    /// Image height in pixels.
    #[reflect(range(min = 1.0, max = 8192.0))]
    pub height: u32,
    /// Which world: edit (default) or play.
    #[serde(default)]
    pub world: Option<WorldSel>,
}

/// `viewport.pick` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportPickResult {
    /// The entity hit, or null.
    pub entity: Option<String>,
    /// Its depth along the view direction from the eye.
    pub distance: Option<f32>,
}

impl Method for ViewportPick {
    const NAME: &'static str = "viewport.pick";
    const MUTATES: bool = false;
    type Params = ViewportPickParams;
    type Result = ViewportPickResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: ViewportPickParams,
    ) -> Result<ViewportPickResult, ApiError> {
        check_view(&p.view)?;
        if p.x >= p.width || p.y >= p.height {
            return Err(ApiError::new(
                ApiErrorKind::InvalidParams,
                format!(
                    "pixel ({}, {}) is outside the {}×{} image",
                    p.x, p.y, p.width, p.height
                ),
            )
            .at("/x"));
        }
        let snapshot = RenderWorld::extract(engine.world(p.world.unwrap_or_default())?);
        let mut memory = MemoryAssets::default();
        let mut bounds = |mesh: &concordia_assets::AssetRef| {
            let data = match engine.state_mut::<AssetDb>() {
                Some(db) => crate::AssetSource::mesh(db, mesh),
                None => crate::AssetSource::mesh(&mut memory, mesh),
            };
            data.ok().map(|m| {
                (
                    concordia_core::math::Vec3::from_array(m.aabb.0),
                    concordia_core::math::Vec3::from_array(m.aabb.1),
                )
            })
        };
        let hit = crate::pick(
            &snapshot,
            &p.view,
            (p.x, p.y),
            (p.width, p.height),
            &mut bounds,
        )
        .map_err(api_error)?;
        Ok(ViewportPickResult {
            entity: hit.map(|h| h.entity.to_string()),
            distance: hit.map(|h| h.depth),
        })
    }
}

/// A view that cannot be used is a parameter error at `/view`.
fn check_view(view: &crate::View) -> Result<(), ApiError> {
    view.world_matrix()
        .and_then(|_| view.projection.matrix(1.0))
        .map(|_| ())
        .map_err(|e| ApiError::new(ApiErrorKind::InvalidParams, e.to_string()).at("/view"))
}

/// Attaches a native window layer as the editor viewport that `viewport.present` draws into
/// (in-process hosts only; run it on the engine thread, e.g. through `EngineHandle::run`).
///
/// # Safety
/// The native object behind `target` must outlive the engine.
pub unsafe fn attach_surface(
    engine: &mut Engine,
    target: concordia_rhi::SurfaceTarget,
) -> Result<(), ApiError> {
    // SAFETY: forwarded from the caller (the engine owns the render thread).
    unsafe { render_thread(engine)?.attach(target) }.map_err(api_error)?;
    if let Some(state) = engine.state_mut::<RenderState>() {
        state.surface = true;
    }
    Ok(())
}

/// Draw `view` into the editor's native viewport (needs a host-attached window surface; elsewhere use capture.screenshot).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportPresent;

/// `viewport.present` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ViewportPresentParams {
    /// The viewpoint.
    pub view: crate::View,
    /// Width in physical pixels.
    #[reflect(range(min = 1.0, max = 8192.0))]
    pub width: u32,
    /// Height in physical pixels.
    #[reflect(range(min = 1.0, max = 8192.0))]
    pub height: u32,
    /// Which world: edit (default) or play.
    #[serde(default)]
    pub world: Option<WorldSel>,
    /// Caller's tag for this view, echoed in `viewport.stats`.
    #[serde(default)]
    pub seq: Option<u64>,
    /// When the input behind this view happened (Unix ms), for input-to-photon latency.
    #[serde(default)]
    pub input_time_ms: Option<f64>,
}

/// `viewport.present` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportPresentResult {
    /// The frame is queued for the render thread, which presents the newest queued view at
    /// display pace (see `viewport.stats` for what reached the screen).
    pub presented: bool,
}

impl Method for ViewportPresent {
    const NAME: &'static str = "viewport.present";
    const MUTATES: bool = false;
    type Params = ViewportPresentParams;
    type Result = ViewportPresentResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: ViewportPresentParams,
    ) -> Result<ViewportPresentResult, ApiError> {
        check_view(&p.view)?;
        if !engine.state::<RenderState>().is_some_and(|s| s.surface) {
            return Err(ApiError::new(
                ApiErrorKind::InvalidRequest,
                "no native viewport is attached",
            )
            .suggest(["use capture.screenshot (the editor streams it in a browser)".to_owned()]));
        }
        let world = RenderWorld::extract(engine.world(p.world.unwrap_or_default())?);
        let assets = ship(engine, &world)?;
        let request = RenderRequest {
            camera: None,
            view: Some(p.view.clone()),
            width: p.width,
            height: p.height,
        };
        let (log, id) = queue_record(engine, &p);
        let frame = ViewportFrame {
            world,
            assets,
            request,
            log: log.clone(),
            id,
        };
        if let Err(e) = render_thread(engine).and_then(|t| t.present(frame).map_err(api_error)) {
            remove_frame(&log, id);
            return Err(e);
        }
        Ok(ViewportPresentResult { presented: true })
    }
}

/// Logs a newly queued native-viewport frame.
fn queue_record(engine: &mut Engine, p: &ViewportPresentParams) -> (FrameLog, u64) {
    let Some(state) = engine.state_mut::<RenderState>() else {
        return (FrameLog::default(), 0);
    };
    state.next_frame += 1;
    let record = FrameRecord {
        seq: p.seq,
        input_time_ms: p.input_time_ms,
        queued_ms: unix_ms(std::time::SystemTime::now()),
        cpu_ms: 0.0,
        submitted_ms: None,
        presented_ms: None,
    };
    push_frame(&state.frames, state.next_frame, record);
    (state.frames.clone(), state.next_frame)
}

/// Recent native-viewport frames: input tag, CPU time, submission and display times.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportStats;

/// `viewport.stats` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ViewportStatsParams {
    /// Empty the record after reading it.
    #[serde(default)]
    pub clear: Option<bool>,
}

/// `viewport.stats` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportStatsResult {
    /// Oldest first (at most 4096).
    pub frames: Vec<FrameRecord>,
}

impl Method for ViewportStats {
    const NAME: &'static str = "viewport.stats";
    const MUTATES: bool = false;
    type Params = ViewportStatsParams;
    type Result = ViewportStatsResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: ViewportStatsParams,
    ) -> Result<ViewportStatsResult, ApiError> {
        let log = engine
            .state::<RenderState>()
            .map(|s| s.frames.clone())
            .unwrap_or_default();
        let mut list = log
            .lock()
            .map_err(|_| ApiError::new(ApiErrorKind::Internal, "viewport frame log poisoned"))?;
        let frames = list.iter().map(|(_, f)| f.clone()).collect();
        if p.clear == Some(true) {
            list.clear();
        }
        Ok(ViewportStatsResult { frames })
    }
}
