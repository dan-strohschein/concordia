//! `capture.reference` through the Engine API (M4 4.1b4): a path-traced image of a world, equal
//! to `scene_from_world` + `render`, capped for agents, falling back to the CPU without a GPU.

use std::sync::Arc;

use base64::Engine as _;
use concordia_api::{ApiErrorKind, Engine, EngineConfig, EngineModule};
use concordia_assets::{AssetDb, AssetsModule};
use concordia_core::id::{DeterministicIdGen, SessionId};
use concordia_pathtrace::{
    Backend, HdrImage, PathTraceModule, Settings, View, render, scene_from_world,
};
use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_render_types::Tonemap;
use serde_json::{Value, json};

/// Registers the render components, as `RenderModule` does in a real project.
#[derive(Debug)]
struct RenderComponents;

impl EngineModule for RenderComponents {
    fn name(&self) -> &'static str {
        "render-components"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        concordia_render_types::register_components(registry)
    }
}

struct Harness {
    engine: Engine,
    session: SessionId,
    _dir: tempfile::TempDir,
}

impl Harness {
    fn new(module: PathTraceModule) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        let modules: Vec<Arc<dyn EngineModule>> = vec![
            Arc::new(AssetsModule::new(dir.path())),
            Arc::new(RenderComponents),
            Arc::new(module),
        ];
        config.modules = modules;
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let scene = json!({"path": "scenes/t.scene.ron", "name": "T", "actor": "agent:test"});
        engine.call(session, "scene.new", scene).unwrap();
        Self {
            engine,
            session,
            _dir: dir,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn spawn(&mut self, components: Value) -> Value {
        self.call(
            "entity.spawn",
            json!({"actor": "agent:test", "components": components}),
        )
        .unwrap()["id"]
            .clone()
    }

    /// A cube under a sun, seen by a camera.
    fn cube_scene(&mut self) {
        self.spawn(json!({
            "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.2, 0.3, 0.0, 0.93], "scale": [1.0, 1.0, 1.0]},
            "MeshRenderer": {"mesh": "builtin:mesh/cube", "material": "builtin:material/default"},
        }));
        self.spawn(json!({
            "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [-0.38, 0.0, 0.0, 0.92], "scale": [1.0, 1.0, 1.0]},
            "DirectionalLight": {"illuminance_lux": 30000.0},
        }));
        self.spawn(json!({
            "Transform": {"translation": [0.0, 0.0, 4.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
            "Camera": {},
        }));
    }
}

fn png_pixels(result: &Value) -> Vec<u8> {
    assert_eq!(result["image"]["mime_type"], "image/png");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(result["image"]["data_base64"].as_str().unwrap())
        .unwrap();
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

#[test]
fn capture_reference_renders_the_world_like_scene_from_world() {
    let mut h = Harness::new(PathTraceModule::new(None));
    h.cube_scene();
    let result = h
        .call(
            "capture.reference",
            json!({"width": 32, "height": 18, "spp": 4, "backend": "cpu"}),
        )
        .unwrap();
    assert_eq!(
        (result["width"].clone(), result["height"].clone()),
        (json!(32), json!(18))
    );
    assert_eq!(result["backend"], "cpu");
    let world = h.engine.world(concordia_api::WorldSel::edit).unwrap();
    let mut db = AssetDb::new(h._dir.path(), Box::new(DeterministicIdGen::new(1))).unwrap();
    let scene = scene_from_world(world, &mut db, &View::Camera(None)).unwrap();
    let settings = Settings {
        width: 32,
        height: 18,
        spp: 4,
        seed: 1,
        max_bounces: 8,
        threads: None,
    };
    let hdr = render(&scene, &settings, &Backend::Cpu).unwrap();
    assert_eq!(
        png_pixels(&result),
        hdr.to_rgba8(scene.camera.tonemap),
        "the same image"
    );
}

#[test]
fn capture_reference_is_capped_for_agents() {
    let mut h = Harness::new(PathTraceModule::new(None));
    h.cube_scene();
    let err = h
        .call(
            "capture.reference",
            json!({"width": 1920, "height": 1080, "spp": 4096}),
        )
        .unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::InvalidParams, "{err:?}");
    assert!(err.message.contains("samples"), "{}", err.message);
    assert!(
        err.data
            .suggestions
            .iter()
            .any(|s| s.contains("bless-reference")),
        "{:?}",
        err.data.suggestions
    );
}

#[test]
fn capture_reference_names_what_is_missing() {
    let mut h = Harness::new(PathTraceModule::new(None));
    let err = h
        .call("capture.reference", json!({"backend": "cpu"}))
        .unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::InvalidParams, "{err:?}");
    assert!(err.message.contains("camera"), "{}", err.message);
}

#[test]
fn without_a_gpu_auto_uses_the_cpu() {
    let mut h = Harness::new(PathTraceModule::new(None));
    h.cube_scene();
    let result = h
        .call(
            "capture.reference",
            json!({"width": 8, "height": 8, "spp": 1}),
        )
        .unwrap();
    assert_eq!(result["backend"], "cpu");
}

#[test]
fn reference_pixels_are_tone_mapped_then_srgb_encoded() {
    let image = HdrImage {
        width: 2,
        height: 1,
        pixels: vec![[0.5, 2.0, 0.0], [0.18, 0.18, 0.18]],
    };
    let linear = image.to_rgba8(Tonemap::Linear);
    assert_eq!(
        &linear[..4],
        &[188, 255, 0, 255],
        "clamped, then sRGB-encoded"
    );
    let agx = image.to_rgba8(Tonemap::AgX);
    assert_ne!(agx[4..8], linear[4..8], "AgX's curve, not a clamp");
}

#[test]
fn capture_reference_traces_from_the_camera_it_is_given() {
    let mut h = Harness::new(PathTraceModule::new(None));
    h.cube_scene();
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let side = h.spawn(json!({
        "Transform": {"translation": [4.0, 0.0, 0.0], "rotation": [0.0, half, 0.0, half], "scale": [1.0, 1.0, 1.0]},
        "Camera": {"priority": -1},
    }));
    let params = |camera: Option<&Value>| {
        let mut p = json!({"width": 16, "height": 16, "spp": 2, "backend": "cpu"});
        if let Some(c) = camera {
            p["camera"] = c.clone();
        }
        p
    };
    let default = png_pixels(&h.call("capture.reference", params(None)).unwrap());
    let from_side = png_pixels(&h.call("capture.reference", params(Some(&side))).unwrap());
    assert_ne!(
        default, from_side,
        "the side camera sees the cube from another angle"
    );
}

#[test]
fn with_a_device_auto_traces_on_the_gpu_and_matches_the_cpu() {
    if concordia_rhi_metal::new_headless().is_err() {
        assert!(
            !concordia_testkit::gpu_required(),
            "CONCORDIA_REQUIRE_GPU=1 but no device"
        );
        return;
    }
    let mut h = Harness::new(PathTraceModule::new(Some(Arc::new(
        concordia_rhi_metal::new_headless,
    ))));
    h.cube_scene();
    let params = |backend: &str| json!({"width": 32, "height": 18, "spp": 64, "backend": backend});
    let gpu = h.call("capture.reference", params("auto")).unwrap();
    assert_eq!(gpu["backend"], "gpu");
    let cpu = h.call("capture.reference", params("cpu")).unwrap();
    let (g, c) = (png_pixels(&gpu), png_pixels(&cpu));
    let mean = |p: &[u8]| p.iter().map(|&v| f64::from(v)).sum::<f64>() / p.len() as f64;
    assert!(
        (mean(&g) - mean(&c)).abs() < 2.0,
        "the same scene: {} vs {}",
        mean(&g),
        mean(&c)
    );
}
