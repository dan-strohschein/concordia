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

/// Engine-lifetime state: the device factory and, after the first capture, the renderer.
struct RenderState {
    factory: DeviceFactory,
    renderer: Option<Renderer>,
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
        vec![MethodInfo::of::<CaptureScreenshot>()]
    }

    fn engine_state(&self) -> Option<Box<dyn std::any::Any + Send>> {
        Some(Box::new(RenderState {
            factory: self.factory.clone(),
            renderer: None,
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
        let world = engine.world(p.world.unwrap_or_default())?;
        let camera = p.camera.as_ref().map(|c| c.resolve(world)).transpose()?;
        let sim_tick = world.resource::<SimClock>().map_or(0, |c| c.tick.0);
        let snapshot = RenderWorld::extract(world);
        let mut renderer = take_renderer(engine)?;
        let request = RenderRequest {
            camera,
            width: p.width,
            height: p.height,
        };
        let rendered = match engine.state_mut::<AssetDb>() {
            Some(db) => renderer.render(&snapshot, db, &request),
            None => renderer.render(&snapshot, &mut MemoryAssets::default(), &request),
        };
        let frame = renderer.frames();
        put_renderer(engine, renderer);
        let image = rendered.map_err(api_error)?;
        let actor = ctx
            .actor
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        tracing::info!(target: "concordia::render", actor, width = p.width, height = p.height, sim_tick, frame, "captured screenshot");
        Ok(CaptureScreenshotResult {
            image: EncodedImage::png(&encode_png(&image)?),
            width: image.width,
            height: image.height,
            sim_tick,
            frame,
        })
    }
}

/// The renderer, created on first use (taken out so assets can be borrowed alongside).
fn take_renderer(engine: &mut Engine) -> Result<Renderer, ApiError> {
    let state = engine
        .state_mut::<RenderState>()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the render module has no state"))?;
    if let Some(renderer) = state.renderer.take() {
        return Ok(renderer);
    }
    let device = (state.factory)().map_err(|e| api_error(RenderError::Gpu(e)))?;
    let toolchain =
        SlangToolchain::locate().map_err(|e| api_error(RenderError::Shader(e.to_string())))?;
    Renderer::new(Arc::from(device), Box::new(SlangcCompiler::new(toolchain))).map_err(api_error)
}

fn put_renderer(engine: &mut Engine, renderer: Renderer) {
    if let Some(state) = engine.state_mut::<RenderState>() {
        state.renderer = Some(renderer);
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
