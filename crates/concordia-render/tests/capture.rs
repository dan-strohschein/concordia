//! `capture.screenshot` through the Engine API (M1 phase 1.5). GPU cases skip without a GPU
//! unless CONCORDIA_REQUIRE_GPU=1.

use std::sync::Arc;

use base64::Engine as _;
use concordia_api::{Engine, EngineConfig, EngineModule, MethodInfo, params_schema};
use concordia_assets::AssetsModule;
use concordia_core::id::SessionId;
use concordia_render::{CaptureScreenshot, DeviceFactory, RenderModule};
use concordia_rhi::RhiError;
use concordia_testkit::gpu_required;
use serde_json::{Value, json};

struct Harness {
    engine: Engine,
    session: SessionId,
    _dir: tempfile::TempDir,
}

impl Harness {
    fn with(factory: DeviceFactory) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        let modules: Vec<Arc<dyn EngineModule>> = vec![
            Arc::new(AssetsModule::new(dir.path())),
            Arc::new(RenderModule::new(factory)),
        ];
        config.modules = modules;
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let scene = json!({"path": "scenes/test.scene.ron", "name": "Test", "actor": "agent:test"});
        engine.call(session, "scene.new", scene).expect("scene.new");
        Self {
            engine,
            session,
            _dir: dir,
        }
    }

    /// A harness on the real GPU, or None (skip) without one unless required.
    fn gpu() -> Option<Self> {
        if let Err(RhiError::NoDevice { reason }) = concordia_rhi_metal::new_headless() {
            assert!(
                !gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            return None;
        }
        Some(Self::with(Arc::new(concordia_rhi_metal::new_headless)))
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    /// A lit cube in front of a camera; returns the camera id.
    fn cube_scene(&mut self) -> Value {
        self.ok("entity.spawn", json!({"actor": "agent:test", "components": {
            "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.2, 0.3, 0.0, 0.93], "scale": [1.0, 1.0, 1.0]},
            "MeshRenderer": {"mesh": "builtin:mesh/cube", "material": "builtin:material/default"},
        }}));
        self.ok("entity.spawn", json!({"actor": "agent:test", "components": {
            "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [-0.38, 0.0, 0.0, 0.92], "scale": [1.0, 1.0, 1.0]},
            "DirectionalLight": {},
        }}));
        let camera = self.ok("entity.spawn", json!({"actor": "agent:test", "components": {
            "Transform": {"translation": [0.0, 0.0, 4.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
            "Camera": {},
        }}));
        camera["id"].clone()
    }
}

fn png_size(result: &Value) -> (u32, u32) {
    assert_eq!(result["image"]["mime_type"], "image/png");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(result["image"]["data_base64"].as_str().unwrap())
        .unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "PNG signature");
    let be = |i: usize| u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap());
    (be(16), be(20))
}

#[test]
fn capture_screenshot_is_the_requested_size() {
    let Some(mut h) = Harness::gpu() else { return };
    h.cube_scene();
    let result = h.ok("capture.screenshot", json!({"width": 320, "height": 200}));
    assert_eq!(png_size(&result), (320, 200));
    assert_eq!(
        (result["width"].as_u64(), result["height"].as_u64()),
        (Some(320), Some(200))
    );
}

#[test]
fn capture_screenshot_defaults_to_1280_by_720() {
    let Some(mut h) = Harness::gpu() else { return };
    h.cube_scene();
    assert_eq!(
        png_size(&h.ok("capture.screenshot", json!({}))),
        (1280, 720)
    );
}

#[test]
fn capture_twice_without_sim_change_is_byte_identical() {
    let Some(mut h) = Harness::gpu() else { return };
    h.cube_scene();
    let a = h.ok("capture.screenshot", json!({"width": 64, "height": 64}));
    let b = h.ok("capture.screenshot", json!({"width": 64, "height": 64}));
    assert_eq!(a["image"], b["image"]);
}

#[test]
fn capture_screenshot_reports_sim_tick_and_frame() {
    let Some(mut h) = Harness::gpu() else { return };
    h.cube_scene();
    let edit = h.ok("capture.screenshot", json!({"width": 16, "height": 16}));
    assert_eq!(edit["sim_tick"], 0);
    h.ok("sim.step", json!({"ticks": 5}));
    let play = h.ok(
        "capture.screenshot",
        json!({"width": 16, "height": 16, "world": "play"}),
    );
    assert_eq!(play["sim_tick"], 5);
    assert_eq!(
        play["frame"].as_u64(),
        edit["frame"].as_u64().map(|f| f + 1),
        "frames count captures"
    );
}

#[test]
fn capture_screenshot_with_unknown_camera_returns_invalid_params() {
    let Some(mut h) = Harness::gpu() else { return };
    h.cube_scene();
    let not_a_camera = h.ok(
        "entity.spawn",
        json!({"actor": "agent:test", "components": {}}),
    )["id"]
        .clone();
    let err = h
        .call("capture.screenshot", json!({"camera": not_a_camera}))
        .unwrap_err();
    assert_eq!(err.data.kind, "InvalidParams", "{err:?}");
    assert!(err.message.contains("Camera"), "{}", err.message);
}

#[test]
fn capture_without_a_gpu_is_gpu_unavailable() {
    let mut h = Harness::with(Arc::new(|| {
        Err(RhiError::NoDevice {
            reason: "test: no GPU".into(),
        })
    }));
    h.cube_scene();
    let err = h.call("capture.screenshot", json!({})).unwrap_err();
    assert_eq!((err.code(), err.data.kind), (-32013, "GpuUnavailable"));
    assert!(err.message.contains("test: no GPU"), "{}", err.message);
}

#[test]
fn render_module_registers_components_and_the_method() {
    let h = Harness::with(Arc::new(|| {
        Err(RhiError::NoDevice {
            reason: "unused".into(),
        })
    }));
    assert!(h.engine.methods().get("capture.screenshot").is_some());
    assert!(h.engine.registry().get_by_name("Camera").is_some());
    let _ = RenderModule::new(Arc::new(concordia_rhi_metal::new_headless)).methods();
}

#[test]
fn capture_screenshot_schema_is_reflect_generated() {
    insta::assert_json_snapshot!(params_schema(&MethodInfo::of::<CaptureScreenshot>()));
}
