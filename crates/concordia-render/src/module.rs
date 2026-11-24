//! The render module: render components and `capture.screenshot` (headless, offscreen).

use std::sync::Arc;

use concordia_api::{
    ApiError, ApiErrorKind, Audience, CallCtx, EncodedImage, Engine, EngineModule, EntityRef,
    Method, MethodInfo, WorldSel,
};
use concordia_assets::AssetDb;

use crate::assets::AssetKind;
use concordia_command::Actor;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_rhi::{Device, RhiError};
use concordia_shaders::{SlangToolchain, SlangcCompiler};
use concordia_terra::SimClock;
use serde::{Deserialize, Serialize};

use std::collections::{BTreeMap, BTreeSet};

use concordia_assets::AssetRef;

pub use crate::frame_log::FrameRecord;
use crate::frame_log::{FrameLog, push_frame, remove_frame, unix_ms};
use crate::orbit::{OrbitCamera, Preset};
use crate::thread::{RenderThread, SceneUpdate, ViewportFrame};
use crate::{Image, MemoryAssets, RenderError, RenderRequest, RenderWorld, Renderer};
use crate::{SceneCache, SceneDelta};

/// Creates the GPU device on first use (so engines start without one).
pub type DeviceFactory = Arc<dyn Fn() -> Result<Box<dyn Device>, RhiError> + Send + Sync>;

/// Registers render components and `capture.screenshot`.
pub struct RenderModule {
    factory: DeviceFactory,
    /// The project's closure budget (None: the platform's).
    closure_budget: Option<crate::ClosureBudget>,
    /// Time each pass of every frame (`render.stats` `passes`).
    perf_mode: bool,
}

impl std::fmt::Debug for RenderModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderModule").finish_non_exhaustive()
    }
}

impl RenderModule {
    /// A module that renders on devices from `factory`.
    pub fn new(factory: DeviceFactory) -> Self {
        Self {
            factory,
            closure_budget: None,
            perf_mode: false,
        }
    }

    /// Turns perf mode on: every frame's passes are timed (`concordia perf run`, perf tests).
    pub fn with_perf_mode(self, perf_mode: bool) -> Self {
        Self { perf_mode, ..self }
    }

    /// Lowers the platform's closure budget to the project's (`[render] closure_budget`); a
    /// budget above the platform's is ignored with a warning.
    pub fn with_closure_budget(self, closure_budget: Option<crate::ClosureBudget>) -> Self {
        Self {
            closure_budget,
            ..self
        }
    }
}

/// Engine-lifetime state: the device factory and, once something renders, the render thread.
pub(crate) struct RenderState {
    factory: DeviceFactory,
    closure_budget: Option<crate::ClosureBudget>,
    perf_mode: bool,
    thread: Option<RenderThread>,
    /// Whether a host attached the editor's native viewport.
    surface: bool,
    /// Meshes and textures already sent to the render thread (it keeps them).
    shipped: BTreeSet<AssetRef>,
    /// Each imported asset sent, with its kind and asset generation then (re-sent when it moves).
    generations: BTreeMap<AssetRef, (AssetKind, u64)>,
    /// What the render thread last saw of the edit and play worlds.
    scenes: [SceneCache; 2],
    /// Recent native-viewport frames (for `viewport.stats`), keyed by frame number.
    frames: FrameLog,
    next_frame: Arc<std::sync::atomic::AtomicU64>,
    /// The editor's camera (`viewport.navigate`).
    pub(crate) camera: SharedCamera,
    /// The native viewport's last size and world, so navigation can re-present it.
    pub(crate) viewport: Option<(u32, u32, Option<WorldSel>)>,
    /// The native viewport's last scale factor (physical per logical pixel).
    pub(crate) viewport_scale: f32,
    /// What the native viewport is drawn from (the editor camera, or a scene camera).
    pub(crate) source: Source,
    /// What the editor draws over the viewport (`viewport.overlays`).
    pub(crate) overlays: crate::Overlays,
    /// Where the gizmo is (its entity and world position), shared with `ViewportControl`.
    pub(crate) anchor: GizmoAnchor,
    /// The gizmo drag in progress (`viewport.gizmo`).
    pub(crate) gizmo: Option<crate::gizmo::GizmoDrag>,
    /// The last frame drawn (`render.stats`), recorded by the render thread.
    pub(crate) last: crate::stats::LastFrame,
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

    fn templates(&self) -> Vec<concordia_api::Template> {
        crate::templates::templates()
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![
            MethodInfo::of::<CaptureScreenshot>(),
            MethodInfo::of::<ViewportPick>(),
            MethodInfo::of::<ViewportDrop>(),
            MethodInfo::of::<ViewportPresent>(),
            MethodInfo::of::<ViewportStats>(),
            MethodInfo::of::<ViewportNavigate>(),
            MethodInfo::of::<crate::pointer::ViewportPointerRay>(),
            MethodInfo::of::<ViewportOverlays>(),
            MethodInfo::of::<crate::gizmo::ViewportGizmo>(),
            MethodInfo::of::<crate::stats::RenderStatsGet>(),
        ]
    }

    /// A native viewport showing the play world redraws on every simulation tick.
    fn after_sim(&self, engine: &mut Engine) {
        let Some(state) = engine.state::<RenderState>() else {
            return;
        };
        let Some((width, height, Some(WorldSel::play))) = state.viewport.filter(|_| state.surface)
        else {
            return;
        };
        let look = look_for(&state.source, state.camera.view());
        let scale = state.viewport_scale;
        let target = PresentTarget {
            width,
            height,
            world: Some(WorldSel::play),
            seq: None,
            input_time_ms: None,
            tick: play_tick(engine),
            scale,
        };
        if let Err(e) = queue_present(engine, look, target) {
            tracing::warn!(target: "concordia::render", error = %e.message, "play viewport redraw failed");
        }
    }

    fn engine_state(&self) -> Option<Box<dyn std::any::Any + Send>> {
        Some(Box::new(RenderState {
            factory: self.factory.clone(),
            closure_budget: self.closure_budget,
            perf_mode: self.perf_mode,
            thread: None,
            surface: false,
            shipped: BTreeSet::new(),
            generations: BTreeMap::new(),
            scenes: Default::default(),
            frames: FrameLog::default(),
            next_frame: Arc::default(),
            camera: SharedCamera::default(),
            viewport: None,
            viewport_scale: 1.0,
            source: Source::Editor,
            overlays: crate::Overlays::default(),
            anchor: GizmoAnchor::default(),
            gizmo: None,
            last: Default::default(),
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
    /// Width in pixels (default 768 × 432, sized for an agent's context; ask for more when detail matters).
    #[serde(default = "default_width")]
    #[reflect(default = 768, range(min = 1.0, max = 8192.0))]
    pub width: u32,
    /// Height in pixels.
    #[serde(default = "default_height")]
    #[reflect(default = 432, range(min = 1.0, max = 8192.0))]
    pub height: u32,
    /// Which world to render: edit (default) or play.
    #[serde(default)]
    pub world: Option<WorldSel>,
    /// Who is asking (attribution in the log only).
    #[serde(default)]
    pub actor: Option<Actor>,
    /// Draw a debug view instead of the lit image, e.g. "wireframe" or "normals" (see ViewMode).
    #[serde(default)]
    pub view_mode: Option<crate::ViewMode>,
    /// Draw the editor overlays (`viewport.overlays`: selection, gizmo) on top.
    #[serde(default)]
    pub overlays: Option<bool>,
    /// Draw every posed skeleton's bones on top (play world), whatever the editor overlays are.
    #[serde(default)]
    pub skeleton: Option<bool>,
    /// Debug layers to draw on top, e.g. ["navmesh"]; an unknown name's error lists the layers.
    #[serde(default)]
    pub debug: Option<Vec<String>>,
    /// Draw the in-game UI (default true).
    #[serde(default)]
    pub ui: Option<bool>,
    /// The UI's pixels per logical pixel (0.25..=8; default 1).
    #[serde(default)]
    #[reflect(range(min = 0.25, max = 8.0))]
    pub ui_scale: Option<f32>,
}

fn default_width() -> u32 {
    768
}

/// What a capture draws on top: the editor overlays if asked, plus the skeletons if asked.
fn capture_overlays(engine: &Engine, p: &CaptureScreenshotParams) -> Option<crate::Overlays> {
    let mut overlays = (p.overlays == Some(true))
        .then(|| engine.state::<RenderState>().map(|s| s.overlays.clone()))
        .flatten();
    if p.skeleton == Some(true) {
        overlays
            .get_or_insert_with(crate::Overlays::default)
            .skeleton = true;
    }
    overlays
}

fn default_height() -> u32 {
    432
}

/// Unknown debug layer names, as an `InvalidParams` error listing the known ones.
fn check_layers(engine: &Engine, names: &[String]) -> Result<(), ApiError> {
    let known = engine.debug_layers();
    match names.iter().find(|n| !known.contains(&n.as_str())) {
        Some(name) => Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("unknown debug layer {name:?}; the layers are {known:?}"),
        )
        .at("/debug")),
        None => Ok(()),
    }
}

/// The geometry of each named layer (once each, in order) in `world`; unknown names are skipped.
fn layer_geometry(
    engine: &Engine,
    names: &[String],
    world: WorldSel,
) -> Vec<concordia_api::DebugGeometry> {
    let mut seen = std::collections::BTreeSet::new();
    names
        .iter()
        .filter(|n| seen.insert(n.as_str()))
        .filter_map(|n| engine.debug_geometry(n, world))
        .collect()
}

/// A capture's in-game UI: none when `ui` is false, else every module's at `ui_scale`.
fn capture_ui(
    engine: &mut Engine,
    p: &CaptureScreenshotParams,
) -> Result<concordia_api::UiDrawList, ApiError> {
    if p.ui == Some(false) {
        return Ok(concordia_api::UiDrawList::default());
    }
    let scale = p.ui_scale.unwrap_or(1.0);
    if !(scale.is_finite() && (0.25..=8.0).contains(&scale)) {
        return Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("ui_scale {scale} must be in [0.25, 8]"),
        )
        .at("/ui_scale"));
    }
    let layers = capture_layers(engine, p);
    let world = p.world.unwrap_or_default();
    let clip_from_world = {
        let w = engine.world(world)?;
        let requested = p.camera.as_ref().map(|c| c.resolve(w)).transpose()?;
        crate::pointer::ui_camera(w, requested, p.view.as_ref(), p.width, p.height)
    };
    let frame = concordia_api::UiFrame {
        width: p.width,
        height: p.height,
        scale,
        bounds: layers.iter().any(|l| l == concordia_api::UI_BOUNDS_LAYER),
        clip_from_world,
    };
    Ok(engine.ui_draw(world, &frame))
}

/// A capture's layers: its own `debug`, plus the viewport's when it draws the editor overlays.
fn capture_layers(engine: &Engine, p: &CaptureScreenshotParams) -> Vec<String> {
    let mut names = p.debug.clone().unwrap_or_default();
    if p.overlays == Some(true)
        && let Some(state) = engine.state::<RenderState>()
    {
        names.extend(state.overlays.debug.iter().cloned());
    }
    names
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
        check_layers(engine, p.debug.as_deref().unwrap_or_default())?;
        let debug = layer_geometry(
            engine,
            &capture_layers(engine, &p),
            p.world.unwrap_or_default(),
        );
        let ui = capture_ui(engine, &p)?;
        let world = engine.world(p.world.unwrap_or_default())?;
        let camera = p.camera.as_ref().map(|c| c.resolve(world)).transpose()?;
        let sim_tick = world.resource::<SimClock>().map_or(0, |c| c.tick.0);
        let request = RenderRequest {
            camera,
            view: p.view.clone(),
            width: p.width,
            height: p.height,
            overlays: capture_overlays(engine, &p),
            view_mode: p.view_mode.unwrap_or_default(),
            debug,
            ui,
        };
        let scene = scene_update(engine, p.world.unwrap_or_default(), None)?;
        let slot = scene.slot;
        let captured =
            render_thread(engine).and_then(|t| t.capture(scene, request).map_err(api_error));
        let (image, frame) = captured.inspect_err(|_| forget_scene(engine, slot))?;
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
    let ddc = engine.project().join(".concordia/ddc");
    let state = engine
        .state_mut::<RenderState>()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the render module has no state"))?;
    if state.thread.is_none() {
        let (factory, budget, perf) =
            (state.factory.clone(), state.closure_budget, state.perf_mode);
        let last = state.last.clone();
        state.thread = Some(
            RenderThread::spawn(move || make_renderer(&factory, ddc, budget, perf), last)
                .map_err(api_error)?,
        );
    }
    state
        .thread
        .as_ref()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the render thread did not start"))
}

/// A renderer whose compiled shaders are cached in the project's DDC (`ddc`), so later starts
/// skip the shader compiler, with the project's closure budget.
fn make_renderer(
    factory: &DeviceFactory,
    ddc: std::path::PathBuf,
    closure_budget: Option<crate::ClosureBudget>,
    perf_mode: bool,
) -> Result<Renderer, RenderError> {
    let device = factory().map_err(RenderError::Gpu)?;
    let toolchain = SlangToolchain::locate().map_err(|e| RenderError::Shader(e.to_string()))?;
    let compiler = concordia_shaders::CachedCompiler::new(
        SlangcCompiler::new(toolchain),
        concordia_core::ddc::Ddc::new(ddc),
    );
    let mut renderer = Renderer::new(Arc::from(device), Box::new(compiler))?;
    renderer.set_closure_budget(closure_budget);
    renderer.set_perf_mode(perf_mode);
    Ok(renderer)
}

/// The changes to world `sel` since the render thread last saw it, with the asset data they need.
/// On error the scene is forgotten, so the next update is complete.
fn scene_update(
    engine: &mut Engine,
    sel: WorldSel,
    tick: Option<std::time::Duration>,
) -> Result<SceneUpdate, ApiError> {
    let slot = match sel {
        WorldSel::edit => 0,
        WorldSel::play => 1,
    };
    let mut cache = engine
        .state_mut::<RenderState>()
        .map(|s| std::mem::take(&mut s.scenes[slot]))
        .unwrap_or_default();
    let delta = engine.world(sel).map(|w| cache.update(w));
    if let Some(state) = engine.state_mut::<RenderState>() {
        state.scenes[slot] = cache;
    }
    let delta = delta?;
    let changed = match &delta {
        SceneDelta::Full(items) => RenderWorld::assemble(items.values()),
        SceneDelta::Changes { upserts, .. } => {
            RenderWorld::assemble(upserts.iter().map(|(_, i)| i))
        }
    };
    match ship(engine, &changed) {
        Ok(assets) => Ok(SceneUpdate {
            slot,
            delta,
            assets,
            tick,
        }),
        Err(e) => {
            forget_scene(engine, slot);
            Err(e)
        }
    }
}

/// Drops what the render thread is believed to hold of a world (its next update is complete).
fn forget_scene(engine: &mut Engine, slot: usize) {
    if let Some(state) = engine.state_mut::<RenderState>() {
        state.scenes[slot] = SceneCache::default();
    }
}

/// The asset data `world` needs that the render thread has not been sent yet, plus imported
/// meshes, materials and textures whose content changed since they were sent (hot reload,
/// `material.set`, `asset.import`).
fn ship(engine: &mut Engine, world: &RenderWorld) -> Result<MemoryAssets, ApiError> {
    let (shipped, mut generations) = engine
        .state_mut::<RenderState>()
        .map(|s| {
            (
                std::mem::take(&mut s.shipped),
                std::mem::take(&mut s.generations),
            )
        })
        .unwrap_or_default();
    let held = |r: &AssetRef| shipped.contains(r);
    let resolved = match engine.state_mut::<AssetDb>() {
        Some(db) => {
            let stale = stale_assets(&generations, db);
            crate::resolve(world, db, &held).and_then(|mut assets| {
                let changed =
                    crate::assets::reload(&stale, db, &|r| held(r) || assets.contains(r))?;
                assets.merge(changed);
                note_generations(&assets, db, &mut generations);
                Ok(assets)
            })
        }
        None => crate::resolve(world, &mut MemoryAssets::default(), &held),
    };
    let mut shipped = shipped;
    if let Ok(assets) = &resolved {
        shipped.extend(assets.refs().cloned());
    }
    if let Some(state) = engine.state_mut::<RenderState>() {
        state.shipped = shipped;
        state.generations = generations;
    }
    resolved.map_err(api_error)
}

/// Sent assets whose asset was re-imported with new content.
fn stale_assets(
    generations: &BTreeMap<AssetRef, (AssetKind, u64)>,
    db: &AssetDb,
) -> Vec<(AssetRef, AssetKind)> {
    generations
        .iter()
        .filter(|(r, (_, sent))| asset_id(r).is_some_and(|id| db.generation(id) != *sent))
        .map(|(r, (kind, _))| (r.clone(), *kind))
        .collect()
}

/// Records the kind and generation of every imported asset in `assets`.
fn note_generations(
    assets: &MemoryAssets,
    db: &AssetDb,
    into: &mut BTreeMap<AssetRef, (AssetKind, u64)>,
) {
    for (reference, kind) in assets.kinds() {
        if let Some(id) = asset_id(reference) {
            into.insert(reference.clone(), (kind, db.generation(id)));
        }
    }
}

fn asset_id(reference: &AssetRef) -> Option<concordia_core::id::AssetId> {
    match reference {
        AssetRef::Asset { id, .. } => Some(*id),
        AssetRef::Builtin { .. } => None,
    }
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
pub(crate) fn api_error(error: RenderError) -> ApiError {
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
        check_pixel(p.x, p.y, p.width, p.height)?;
        let hit = with_bounds(engine, p.world, |snapshot, bounds| {
            crate::pick(snapshot, &p.view, (p.x, p.y), (p.width, p.height), bounds)
        })?;
        Ok(ViewportPickResult {
            entity: hit.map(|h| h.entity.to_string()),
            distance: hit.map(|h| h.depth),
        })
    }
}

/// Where an item dropped at pixel (x, y) of a width×height image from `view` lands: on the mesh under it (bounds-based), else on the ground plane (y = 0), else at the view's focus distance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportDrop;

/// `viewport.drop` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportDropResult {
    /// The landing point in world space.
    pub position: [f32; 3],
    /// The entity it lands on, or null (the ground or empty space).
    pub entity: Option<String>,
}

impl Method for ViewportDrop {
    const NAME: &'static str = "viewport.drop";
    const MUTATES: bool = false;
    type Params = ViewportPickParams;
    type Result = ViewportDropResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: ViewportPickParams,
    ) -> Result<ViewportDropResult, ApiError> {
        check_view(&p.view)?;
        check_pixel(p.x, p.y, p.width, p.height)?;
        let landing = with_bounds(engine, p.world, |snapshot, bounds| {
            crate::landing(snapshot, &p.view, (p.x, p.y), (p.width, p.height), bounds)
        })?;
        Ok(ViewportDropResult {
            position: landing.point.to_array(),
            entity: landing.entity.map(|e| e.to_string()),
        })
    }
}

/// A pixel outside the image is a parameter error at `/x`.
fn check_pixel(x: u32, y: u32, width: u32, height: u32) -> Result<(), ApiError> {
    match x < width && y < height {
        true => Ok(()),
        false => Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("pixel ({x}, {y}) is outside the {width}×{height} image"),
        )
        .at("/x")),
    }
}

/// Runs `f` on a snapshot of `world` with a mesh-bounds lookup (the project's assets, else built-ins).
fn with_bounds<R>(
    engine: &mut Engine,
    world: Option<WorldSel>,
    f: impl FnOnce(
        &RenderWorld,
        &mut dyn FnMut(&AssetRef) -> Option<(Vec3, Vec3)>,
    ) -> Result<R, RenderError>,
) -> Result<R, ApiError> {
    let snapshot = RenderWorld::extract(engine.world(world.unwrap_or_default())?);
    let mut memory = MemoryAssets::default();
    let mut bounds = |mesh: &AssetRef| {
        let data = match engine.state_mut::<AssetDb>() {
            Some(db) => crate::AssetSource::mesh(db, mesh),
            None => crate::AssetSource::mesh(&mut memory, mesh),
        };
        data.ok()
            .map(|m| (Vec3::from_array(m.aabb.0), Vec3::from_array(m.aabb.1)))
    };
    f(&snapshot, &mut bounds).map_err(api_error)
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
    /// The editor's viewpoint; without one, the scene camera `camera` (or the highest-priority one).
    #[serde(default)]
    pub view: Option<crate::View>,
    /// Camera entity to draw from when there is no `view` (default: the highest-priority Camera).
    #[serde(default)]
    pub camera: Option<EntityRef>,
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
    /// Physical pixels per logical pixel (the window's scale factor, 0.25..=8; default 1), so
    /// the in-game UI draws at the display's density.
    #[serde(default)]
    #[reflect(range(min = 0.25, max = 8.0))]
    pub scale: Option<f32>,
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
    const AUDIENCE: Audience = Audience::editor;
    type Params = ViewportPresentParams;
    type Result = ViewportPresentResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: ViewportPresentParams,
    ) -> Result<ViewportPresentResult, ApiError> {
        let look = present_look(engine, &p)?;
        if !engine.state::<RenderState>().is_some_and(|s| s.surface) {
            return Err(ApiError::new(
                ApiErrorKind::InvalidRequest,
                "no native viewport is attached",
            )
            .suggest(["use capture.screenshot (the editor streams it in a browser)".to_owned()]));
        }
        let target = PresentTarget {
            width: p.width,
            height: p.height,
            world: p.world,
            seq: p.seq,
            input_time_ms: p.input_time_ms,
            tick: None,
            scale: present_scale(p.scale)?,
        };
        queue_present(engine, look, target)?;
        Ok(ViewportPresentResult { presented: true })
    }
}

/// Where and why a native-viewport frame is drawn.
pub(crate) struct PresentTarget {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) world: Option<WorldSel>,
    pub(crate) seq: Option<u64>,
    pub(crate) input_time_ms: Option<f64>,
    /// Set for redraws after a simulation tick of this length.
    pub(crate) tick: Option<std::time::Duration>,
    /// Physical pixels per logical pixel (the window's scale factor), for the UI.
    pub(crate) scale: f32,
}

/// Redraws the attached native viewport (if any) at its last size, tagged with an input.
pub(crate) fn redraw_viewport(
    engine: &mut Engine,
    seq: Option<u64>,
    input_time_ms: Option<f64>,
) -> Result<(), ApiError> {
    let Some(state) = engine.state::<RenderState>() else {
        return Ok(());
    };
    let Some((width, height, world)) = state.viewport.filter(|_| state.surface) else {
        return Ok(());
    };
    let look = look_for(&state.source, state.camera.view());
    let target = PresentTarget {
        width,
        height,
        world,
        seq,
        input_time_ms,
        tick: None,
        scale: state.viewport_scale,
    };
    queue_present(engine, look, target)
}

/// A present's UI scale: 1 unless given, refused outside 0.25..=8.
fn present_scale(scale: Option<f32>) -> Result<f32, ApiError> {
    let scale = scale.unwrap_or(1.0);
    match scale.is_finite() && (0.25..=8.0).contains(&scale) {
        true => Ok(scale),
        false => Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("scale {scale} must be in [0.25, 8]"),
        )
        .at("/scale")),
    }
}

/// The viewport's debug layers (`viewport.overlays`).
fn viewport_layers(engine: &Engine) -> Vec<String> {
    engine
        .state::<RenderState>()
        .map(|s| s.overlays.debug.clone())
        .unwrap_or_default()
}

/// Snapshots the world and queues a frame of `view` on the render thread.
pub(crate) fn queue_present(
    engine: &mut Engine,
    look: Look,
    t: PresentTarget,
) -> Result<(), ApiError> {
    refresh_anchor(engine);
    let scene = scene_update(engine, t.world.unwrap_or_default(), t.tick)?;
    let slot = scene.slot;
    let (camera, view, source) = match look {
        Look::View(view) => (None, Some(view), Source::Editor),
        Look::Camera(id) => (id, None, Source::Scene(id)),
    };
    let layers = viewport_layers(engine);
    let clip_from_world = engine
        .world(t.world.unwrap_or_default())
        .ok()
        .and_then(|w| crate::pointer::ui_camera(w, camera, view.as_ref(), t.width, t.height));
    let ui_frame = concordia_api::UiFrame {
        width: t.width,
        height: t.height,
        scale: t.scale,
        bounds: layers.iter().any(|l| l == concordia_api::UI_BOUNDS_LAYER),
        clip_from_world,
    };
    let ui = engine.ui_draw(t.world.unwrap_or_default(), &ui_frame);
    let request = RenderRequest {
        camera,
        view,
        width: t.width,
        height: t.height,
        overlays: engine.state::<RenderState>().map(|s| s.overlays.clone()),
        view_mode: engine
            .state::<RenderState>()
            .map(|s| s.overlays.view_mode)
            .unwrap_or_default(),
        debug: layer_geometry(engine, &layers, t.world.unwrap_or_default()),
        ui,
    };
    let (log, id) = queue_record(engine, &t);
    if let Some(state) = engine.state_mut::<RenderState>() {
        state.viewport = Some((t.width, t.height, t.world));
        state.viewport_scale = t.scale;
        state.source = source;
    }
    let frame = ViewportFrame {
        scene,
        request,
        log: log.clone(),
        id,
    };
    if let Err(e) = render_thread(engine).and_then(|t| t.present(frame).map_err(api_error)) {
        remove_frame(&log, id);
        forget_scene(engine, slot);
        return Err(e);
    }
    Ok(())
}

/// Logs a newly queued native-viewport frame.
fn queue_record(engine: &mut Engine, p: &PresentTarget) -> (FrameLog, u64) {
    let Some(state) = engine.state_mut::<RenderState>() else {
        return (FrameLog::default(), 0);
    };
    let id = state
        .next_frame
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + 1;
    let record = FrameRecord {
        seq: p.seq,
        input_time_ms: p.input_time_ms,
        queued_ms: unix_ms(std::time::SystemTime::now()),
        cpu_ms: 0.0,
        submitted_ms: None,
        presented_ms: None,
        gpu_ms: None,
    };
    push_frame(&state.frames, id, record);
    (state.frames.clone(), id)
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
    const AUDIENCE: Audience = Audience::editor;
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

/// Move the editor's camera (orbit, pan, zoom, presets, focus) and return its view; the native
/// viewport, if attached, is redrawn. Pass no fields to read the current view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportNavigate;

/// `viewport.navigate` params (applied in order: preset, toggle_ortho, focus, orbit, pan, zoom).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ViewportNavigateParams {
    /// Jump to a named view.
    #[serde(default)]
    pub preset: Option<Preset>,
    /// Switch between perspective and orthographic.
    #[serde(default)]
    pub toggle_ortho: Option<bool>,
    /// Orbit around this point from now on.
    #[serde(default)]
    pub focus: Option<concordia_core::math::Vec3>,
    /// Orbit by a pointer drag of [dx, dy] pixels.
    #[serde(default)]
    pub orbit: Option<[f32; 2]>,
    /// Pan by a pointer drag of [dx, dy] pixels (needs `height`).
    #[serde(default)]
    pub pan: Option<[f32; 2]>,
    /// Viewport height in pixels, for panning.
    #[serde(default)]
    #[reflect(range(min = 1.0, max = 16384.0))]
    pub height: Option<f32>,
    /// Zoom steps: positive zooms out.
    #[serde(default)]
    pub zoom: Option<f32>,
    /// Caller's input tag, echoed in `viewport.stats` for the redrawn frame.
    #[serde(default)]
    pub seq: Option<u64>,
    /// When the input happened (Unix ms).
    #[serde(default)]
    pub input_time_ms: Option<f64>,
}

/// `viewport.navigate` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportNavigateResult {
    /// The camera's view after the move.
    pub view: crate::View,
}

impl Method for ViewportNavigate {
    const NAME: &'static str = "viewport.navigate";
    const MUTATES: bool = false;
    const AUDIENCE: Audience = Audience::editor;
    type Params = ViewportNavigateParams;
    type Result = ViewportNavigateResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: ViewportNavigateParams,
    ) -> Result<ViewportNavigateResult, ApiError> {
        Ok(ViewportNavigateResult {
            view: navigate(engine, &p)?,
        })
    }
}

/// Applies `p` to the editor camera, redraws the native viewport if one is attached, and returns
/// the new view (for in-process hosts' native input, and `viewport.navigate`).
pub fn navigate(engine: &mut Engine, p: &ViewportNavigateParams) -> Result<crate::View, ApiError> {
    let state = engine
        .state_mut::<RenderState>()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the render module has no state"))?;
    let view = state.camera.apply(p)?;
    let redraw = state.surface.then_some(state.viewport).flatten();
    if let Some((width, height, world)) = redraw {
        let target = PresentTarget {
            width,
            height,
            world,
            seq: p.seq,
            input_time_ms: p.input_time_ms,
            tick: None,
            scale: state.viewport_scale,
        };
        queue_present(engine, Look::View(view.clone()), target)?;
    }
    Ok(view)
}

/// The editor camera, shared with [`ViewportControl`] (which moves it off the engine thread).
#[derive(Debug, Clone, Default)]
pub(crate) struct SharedCamera(Arc<std::sync::Mutex<OrbitCamera>>);

impl SharedCamera {
    /// The current view.
    pub(crate) fn view(&self) -> crate::View {
        self.0
            .lock()
            .map(|c| c.view())
            .unwrap_or_else(|e| e.into_inner().view())
    }

    /// Applies a navigation (in order: preset, toggle_ortho, focus, orbit, pan, zoom).
    fn apply(&self, p: &ViewportNavigateParams) -> Result<crate::View, ApiError> {
        let pan_height = match (p.pan, p.height) {
            (Some(_), Some(h)) if h.is_finite() && h >= 1.0 => h,
            (Some(_), _) => {
                return Err(ApiError::new(
                    ApiErrorKind::InvalidParams,
                    "pan needs the viewport height in pixels (≥ 1)",
                )
                .at("/height"));
            }
            _ => 1.0,
        };
        let mut camera = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(preset) = p.preset {
            camera.preset(preset);
        }
        if p.toggle_ortho == Some(true) {
            camera.ortho = !camera.ortho;
        }
        if let Some(point) = p.focus.filter(|v| v.is_finite()) {
            camera.target = point;
        }
        if let Some([dx, dy]) = p.orbit {
            camera.orbit(dx, dy);
        }
        if let Some([dx, dy]) = p.pan {
            camera.pan(dx, dy, pan_height);
        }
        if let Some(steps) = p.zoom {
            camera.zoom(steps);
        }
        Ok(camera.view())
    }
}

/// Moves the editor camera and redraws the native viewport without the engine thread, so a busy
/// engine (a long agent query, a big transaction) never delays navigation. The render thread
/// redraws its latest scene; world changes still arrive through the engine as usual.
#[derive(Clone)]
pub struct ViewportControl {
    camera: SharedCamera,
    anchor: GizmoAnchor,
    redraw: crate::thread::Redrawer,
    frames: FrameLog,
    next_frame: Arc<std::sync::atomic::AtomicU64>,
}

impl std::fmt::Debug for ViewportControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportControl").finish_non_exhaustive()
    }
}

impl ViewportControl {
    /// The gizmo handle under (x, y) in a `width`×`height` viewport (points), with its entity.
    pub fn gizmo_hit(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Option<(concordia_core::id::EntityId, crate::GizmoAxis)> {
        let (entity, at, mode) = (*self.anchor.lock().ok()?)?;
        crate::gizmo_pick(mode, &self.camera.view(), at, [x, y], width, height)
            .map(|axis| (entity, axis))
    }

    /// Like `viewport.navigate`, from any thread.
    pub fn navigate(&self, p: &ViewportNavigateParams) -> Result<crate::View, ApiError> {
        let view = self.camera.apply(p)?;
        let id = self
            .next_frame
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        let record = FrameRecord {
            seq: p.seq,
            input_time_ms: p.input_time_ms,
            queued_ms: unix_ms(std::time::SystemTime::now()),
            cpu_ms: 0.0,
            submitted_ms: None,
            presented_ms: None,
            gpu_ms: None,
        };
        push_frame(&self.frames, id, record);
        if self
            .redraw
            .redraw(view.clone(), self.frames.clone(), id)
            .is_err()
        {
            remove_frame(&self.frames, id);
        }
        Ok(view)
    }
}

/// The engine's [`ViewportControl`] (starting the render thread if needed).
pub fn viewport_control(engine: &mut Engine) -> Result<ViewportControl, ApiError> {
    let redraw = render_thread(engine)?.redrawer();
    let state = engine
        .state::<RenderState>()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the render module has no state"))?;
    Ok(ViewportControl {
        camera: state.camera.clone(),
        anchor: state.anchor.clone(),
        redraw,
        frames: state.frames.clone(),
        next_frame: state.next_frame.clone(),
    })
}

/// The play world's tick length, if it has a clock.
fn play_tick(engine: &Engine) -> Option<std::time::Duration> {
    let clock = engine.world(WorldSel::play).ok()?.resource::<SimClock>()?;
    Some(std::time::Duration::from_secs_f64(
        1.0 / f64::from(clock.rate.hz()),
    ))
}

/// Set what the engine draws over the editor viewport (selection outlines, the gizmo, the grid)
/// and its view mode. The native viewport redraws; captures include the overlays with
/// `overlays: true`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportOverlays;

/// `viewport.overlays` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportOverlaysResult {
    /// The overlays now drawn.
    pub overlays: crate::Overlays,
}

impl Method for ViewportOverlays {
    const NAME: &'static str = "viewport.overlays";
    const MUTATES: bool = false;
    const AUDIENCE: Audience = Audience::editor;
    type Params = crate::Overlays;
    type Result = ViewportOverlaysResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: crate::Overlays,
    ) -> Result<ViewportOverlaysResult, ApiError> {
        check_layers(engine, &p.debug)?;
        let state = engine.state_mut::<RenderState>().ok_or_else(|| {
            ApiError::new(ApiErrorKind::Internal, "the render module has no state")
        })?;
        state.overlays = p.clone();
        refresh_anchor(engine);
        redraw_viewport(engine, None, None)?;
        Ok(ViewportOverlaysResult { overlays: p })
    }
}

/// The gizmo's entity, world position and mode, shared with [`ViewportControl`] for native
/// hit-tests.
pub(crate) type GizmoAnchor = Arc<
    std::sync::Mutex<
        Option<(
            concordia_core::id::EntityId,
            concordia_core::math::Vec3,
            crate::GizmoMode,
        )>,
    >,
>;

/// Updates the shared gizmo anchor from the overlays and the viewport's world.
pub(crate) fn refresh_anchor(engine: &mut Engine) {
    let Some(state) = engine.state::<RenderState>() else {
        return;
    };
    let (anchor, gizmo) = (state.anchor.clone(), state.overlays.gizmo.clone());
    let sel = state.viewport.and_then(|(_, _, w)| w).unwrap_or_default();
    let at = gizmo.and_then(|g| {
        let world = engine.world(sel).ok()?;
        let m = concordia_scene::builtin::world_transform(world, g.entity)?;
        Some((
            g.entity,
            m.transform_point3(concordia_core::math::Vec3::ZERO),
            g.mode,
        ))
    });
    if let Ok(mut a) = anchor.lock() {
        *a = at;
    }
}

/// What the native viewport is drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    /// The editor camera (`viewport.navigate`).
    Editor,
    /// A scene camera: this one, or the highest-priority one.
    Scene(Option<concordia_core::id::EntityId>),
}

/// One frame's viewpoint.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Look {
    /// An explicit view (the editor's).
    View(crate::View),
    /// A scene camera: this one, or the highest-priority one.
    Camera(Option<concordia_core::id::EntityId>),
}

/// The viewpoint a redraw uses: the editor's `view`, or the scene camera last presented.
pub(crate) fn look_for(source: &Source, editor: crate::View) -> Look {
    match source {
        Source::Editor => Look::View(editor),
        Source::Scene(id) => Look::Camera(*id),
    }
}

/// `viewport.present`'s viewpoint: its `view`, else its (checked) scene camera.
fn present_look(engine: &Engine, p: &ViewportPresentParams) -> Result<Look, ApiError> {
    match (&p.view, &p.camera) {
        (Some(_), Some(_)) => Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            "pass either view or camera, not both",
        )
        .at("/view")),
        (Some(view), None) => check_view(view).map(|()| Look::View(view.clone())),
        (None, camera) => {
            let world = engine.world(p.world.unwrap_or_default())?;
            let id = camera.as_ref().map(|c| c.resolve(world)).transpose()?;
            crate::pointer::scene_camera(world, id).map_err(api_error)?;
            Ok(Look::Camera(id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraws_keep_the_last_presented_source() {
        let editor = crate::View {
            eye: concordia_core::math::Vec3::Z,
            target: concordia_core::math::Vec3::ZERO,
            projection: crate::Projection::default(),
        };
        assert_eq!(
            look_for(&Source::Editor, editor.clone()),
            Look::View(editor.clone())
        );
        assert_eq!(
            look_for(&Source::Scene(None), editor.clone()),
            Look::Camera(None)
        );
        let id: concordia_core::id::EntityId =
            "018bcfe5-6800-7000-a424-0000000000a0".parse().unwrap();
        assert_eq!(
            look_for(&Source::Scene(Some(id)), editor),
            Look::Camera(Some(id))
        );
    }
}
