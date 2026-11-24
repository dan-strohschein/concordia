//! `capture.reference` (M4 4.1b4, ADR-0090): a path-traced image of a world through the Engine
//! API. Capped for agents (full-quality references are blessed with `just bless-reference`).

use std::sync::Arc;

use concordia_api::{
    ApiError, ApiErrorKind, CallCtx, EncodedImage, Engine, EngineModule, EntityRef, Method,
    MethodInfo, WorldSel,
};
use concordia_assets::{AssetDb, AssetRef, AssetSource};
use concordia_command::Actor;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_render_types::Projection;
use concordia_rhi::{Device, RhiError};
use serde::{Deserialize, Serialize};

use crate::{
    Backend, GpuTracer, LoadedAssets, PathTraceError, Settings, View, asset_refs, render,
    scene_from_world,
};

/// Creates the GPU device on first use (None: CPU only).
pub type DeviceFactory = Arc<dyn Fn() -> Result<Box<dyn Device>, RhiError> + Send + Sync>;

/// Provides `capture.reference`.
pub struct PathTraceModule {
    device: Option<DeviceFactory>,
}

impl std::fmt::Debug for PathTraceModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PathTraceModule").finish_non_exhaustive()
    }
}

impl PathTraceModule {
    /// A module tracing on devices from `device` (None: always the CPU).
    pub fn new(device: Option<DeviceFactory>) -> Self {
        Self { device }
    }
}

impl EngineModule for PathTraceModule {
    fn name(&self) -> &'static str {
        "pathtrace"
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![MethodInfo::of::<CaptureReference>()]
    }

    fn engine_state(&self) -> Option<Box<dyn std::any::Any + Send>> {
        Some(Box::new(State {
            factory: self.device.clone(),
            tracer: None,
        }))
    }
}

/// Render a reference image of the world with the path tracer (slow, physically based: the truth
/// the real-time image is measured against). Capped; bless full-quality references with
/// `just bless-reference`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct CaptureReference;

/// Where to trace.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum ReferenceBackend {
    /// The GPU when there is one, else the CPU.
    #[default]
    auto,
    /// The GPU (an error without one).
    gpu,
    /// The CPU.
    cpu,
}

/// A free viewpoint (exposure and tone mapping come from the scene's camera).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ReferenceView {
    /// Camera position.
    pub eye: Vec3,
    /// Point looked at.
    pub target: Vec3,
    /// Projection.
    pub projection: Projection,
}

/// `capture.reference` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct CaptureReferenceParams {
    /// Camera entity; default: the highest-priority Camera.
    #[serde(default)]
    pub camera: Option<EntityRef>,
    /// Trace from this viewpoint instead of a camera entity.
    #[serde(default)]
    pub view: Option<ReferenceView>,
    /// Width in pixels (default 384).
    #[serde(default = "default_width")]
    #[reflect(default = 384, range(min = 1.0, max = 4096.0))]
    pub width: u32,
    /// Height in pixels (default 216).
    #[serde(default = "default_height")]
    #[reflect(default = 216, range(min = 1.0, max = 4096.0))]
    pub height: u32,
    /// Samples per pixel (default 64: a preview; noise falls as 1/√spp).
    #[serde(default = "default_spp")]
    #[reflect(default = 64, range(min = 1.0, max = 65536.0))]
    pub spp: u32,
    /// Seed (default 1).
    #[serde(default = "default_seed")]
    pub seed: u32,
    /// Bounces (default 8).
    #[serde(default = "default_bounces")]
    #[reflect(default = 8, range(min = 1.0, max = 64.0))]
    pub max_bounces: u32,
    /// Where to trace (default auto: the GPU when there is one).
    #[serde(default)]
    pub backend: ReferenceBackend,
    /// Which world: edit (default) or play.
    #[serde(default)]
    pub world: Option<WorldSel>,
    /// Who is asking (attribution in the log only).
    #[serde(default)]
    pub actor: Option<Actor>,
}

fn default_width() -> u32 {
    384
}
fn default_height() -> u32 {
    216
}
fn default_spp() -> u32 {
    64
}
fn default_seed() -> u32 {
    1
}
fn default_bounces() -> u32 {
    8
}

/// `capture.reference` result (MCP returns `image` as image content).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct CaptureReferenceResult {
    /// The PNG (tone-mapped like `capture.screenshot`).
    pub image: EncodedImage,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Samples per pixel.
    pub spp: u32,
    /// Where it was traced: gpu or cpu.
    pub backend: ReferenceBackend,
    /// Wall-clock time (ms).
    pub elapsed_ms: f64,
}

impl Method for CaptureReference {
    const NAME: &'static str = "capture.reference";
    const MUTATES: bool = false;
    type Params = CaptureReferenceParams;
    type Result = CaptureReferenceResult;

    fn call(
        engine: &mut Engine,
        _ctx: &CallCtx,
        p: CaptureReferenceParams,
    ) -> Result<CaptureReferenceResult, ApiError> {
        check(&p)?;
        let traced = trace(engine, &p)?;
        Ok(CaptureReferenceResult {
            image: EncodedImage::from_rgba8(traced.width, traced.height, &traced.rgba)
                .map_err(|e| ApiError::new(ApiErrorKind::Internal, e))?,
            width: traced.width,
            height: traced.height,
            spp: p.spp,
            backend: traced.backend,
            elapsed_ms: traced.elapsed_ms,
        })
    }
}

/// A traced, tone-mapped image.
#[derive(Debug, Clone, PartialEq)]
pub struct Traced {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// sRGB RGBA8 pixels, rows from the top.
    pub rgba: Vec<u8>,
    /// Where it was traced.
    pub backend: ReferenceBackend,
    /// Wall-clock time (ms).
    pub elapsed_ms: f64,
}

/// Traces `p` without the agents' cap (`just bless-reference` uses this; the method caps first).
pub fn trace(engine: &mut Engine, p: &CaptureReferenceParams) -> Result<Traced, ApiError> {
    if p.camera.is_some() && p.view.is_some() {
        return Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            "pass either camera or view, not both",
        ));
    }
    let sel = p.world.unwrap_or_default();
    let refs = asset_refs(engine.world(sel)?);
    let mut assets = match engine.state_mut::<AssetDb>() {
        Some(db) => LoadedAssets::load(&refs, db),
        None => LoadedAssets::load(&refs, &mut BuiltinsOnly),
    };
    let world = engine.world(sel)?;
    let view = match (&p.camera, &p.view) {
        (Some(c), _) => View::Camera(Some(c.resolve(world)?)),
        (None, Some(v)) => View::Look {
            eye: v.eye,
            target: v.target,
            projection: v.projection,
        },
        (None, None) => View::Camera(None),
    };
    let scene = scene_from_world(world, &mut assets, &view).map_err(api_error)?;
    let (backend, used) = backend(engine, p.backend)?;
    let settings = Settings {
        width: p.width,
        height: p.height,
        spp: p.spp,
        seed: p.seed,
        max_bounces: p.max_bounces,
        threads: None,
    };
    let start = std::time::Instant::now();
    let image = render(&scene, &settings, &backend).map_err(api_error)?;
    Ok(Traced {
        width: image.width,
        height: image.height,
        rgba: image.to_rgba8(scene.camera.tonemap),
        backend: used,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

/// The most samples (width × height × spp) one call may trace: about a minute of work, so an
/// agent's session never blocks for long.
pub const MAX_SAMPLES: u64 = 1280 * 720 * 64;

fn check(p: &CaptureReferenceParams) -> Result<(), ApiError> {
    let samples = u64::from(p.width) * u64::from(p.height) * u64::from(p.spp);
    if samples > MAX_SAMPLES {
        return Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("{samples} samples (width × height × spp) is more than the {MAX_SAMPLES} one call may trace"),
        )
        .suggest([
            "lower spp, width or height for a preview".to_owned(),
            "bless full-quality references with `just bless-reference <scene>`".to_owned(),
        ]));
    }
    Ok(())
}

/// The backend to trace on, built on first use and kept in the module's state.
fn backend(
    engine: &mut Engine,
    want: ReferenceBackend,
) -> Result<(Backend, ReferenceBackend), ApiError> {
    if want == ReferenceBackend::cpu {
        return Ok((Backend::Cpu, ReferenceBackend::cpu));
    }
    let ddc = engine.project().join(".concordia/ddc");
    let gpu = engine
        .state_mut::<State>()
        .and_then(|state| state.tracer(&ddc));
    match (gpu, want) {
        (Some(gpu), _) => Ok((Backend::Gpu(gpu), ReferenceBackend::gpu)),
        (None, ReferenceBackend::gpu) => Err(ApiError::new(
            ApiErrorKind::GpuUnavailable,
            "no GPU can run the path tracer here",
        )
        .suggest(["use \"backend\": \"cpu\" (slower) or \"auto\"".to_owned()])),
        (None, _) => Ok((Backend::Cpu, ReferenceBackend::cpu)),
    }
}

/// The module's state: the device factory and the GPU tracer once built (or why it couldn't be).
pub(crate) struct State {
    factory: Option<DeviceFactory>,
    tracer: Option<Option<Arc<GpuTracer>>>,
}

impl State {
    fn tracer(&mut self, ddc: &std::path::Path) -> Option<Arc<GpuTracer>> {
        if self.tracer.is_none() {
            self.tracer = Some(self.build(ddc));
        }
        self.tracer.clone().flatten()
    }

    fn build(&self, ddc: &std::path::Path) -> Option<Arc<GpuTracer>> {
        let device = (self.factory.as_ref()?)().ok()?;
        let toolchain = concordia_shaders::SlangToolchain::locate().ok()?;
        let compiler = concordia_shaders::CachedCompiler::new(
            concordia_shaders::SlangcCompiler::new(toolchain),
            concordia_core::ddc::Ddc::new(ddc.to_path_buf()),
        );
        match GpuTracer::new(Arc::from(device), &compiler) {
            Ok(t) => Some(Arc::new(t)),
            Err(e) => {
                tracing::warn!(target: "concordia::pathtrace", error = %e, "the GPU path tracer is unavailable; tracing on the CPU");
                None
            }
        }
    }
}

/// Loads only the built-in assets (engines without an asset database).
struct BuiltinsOnly;

impl AssetSource for BuiltinsOnly {
    fn mesh(&mut self, r: &AssetRef) -> Result<concordia_assets::MeshData, String> {
        match concordia_assets::Builtin::of(r) {
            Some(concordia_assets::BuiltinAsset::Mesh(m)) => Ok(m),
            _ => Err(format!("{r}: no asset database in this engine")),
        }
    }
    fn material(&mut self, r: &AssetRef) -> Result<concordia_assets::Material, String> {
        match concordia_assets::Builtin::of(r) {
            Some(concordia_assets::BuiltinAsset::Material(m)) => Ok(m),
            _ => Err(format!("{r}: no asset database in this engine")),
        }
    }
    fn texture(&mut self, r: &AssetRef) -> Result<concordia_assets::TextureData, String> {
        Err(format!("{r}: no asset database in this engine"))
    }
}

/// Tracer errors as API errors agents can act on.
fn api_error(e: PathTraceError) -> ApiError {
    let message = e.to_string();
    match e {
        PathTraceError::NoCamera => {
            ApiError::new(ApiErrorKind::InvalidParams, "no camera to trace from").suggest([
                "spawn an entity with Transform and Camera components, or pass a view".to_owned(),
            ])
        }
        PathTraceError::Gpu(_) => ApiError::new(ApiErrorKind::Internal, message),
        _ => ApiError::new(ApiErrorKind::InvalidParams, message),
    }
}
