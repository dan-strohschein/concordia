//! `capture.screenshot` and `render.stats` through the Engine API (M1 phase 1.5; the closure
//! budget, M2 2.3b). GPU cases skip without a GPU unless CONCORDIA_REQUIRE_GPU=1.

use std::sync::Arc;

use base64::Engine as _;
use concordia_api::{Engine, EngineConfig, EngineModule, MethodInfo, params_schema};
use concordia_assets::{AssetsModule, read_material, write_material};
use concordia_core::id::SessionId;
use concordia_core::log::LogRing;
use concordia_material::{ClosureBudget, fit_budget};
use concordia_render::{CaptureScreenshot, DeviceFactory, RenderModule};
use concordia_rhi::RhiError;
use concordia_testkit::gpu_required;
use serde_json::{Value, json};

struct Harness {
    engine: Engine,
    session: SessionId,
    dir: tempfile::TempDir,
}

impl Harness {
    fn with(factory: DeviceFactory) -> Self {
        Self::configured(RenderModule::new(factory), None)
    }

    /// An engine whose render module is `render`, logging into `ring` when given.
    fn configured(render: RenderModule, ring: Option<Arc<LogRing>>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.log_ring = ring;
        let modules: Vec<Arc<dyn EngineModule>> =
            vec![Arc::new(AssetsModule::new(dir.path())), Arc::new(render)];
        config.modules = modules;
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let scene = json!({"path": "scenes/test.scene.ron", "name": "Test", "actor": "agent:test"});
        engine.call(session, "scene.new", scene).expect("scene.new");
        Self {
            engine,
            session,
            dir,
        }
    }

    /// A harness on the real GPU, or None (skip) without one unless required.
    fn gpu() -> Option<Self> {
        has_gpu().then(|| Self::with(Arc::new(concordia_rhi_metal::new_headless)))
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

/// Whether a GPU is present (failing when one is required but missing).
fn has_gpu() -> bool {
    if let Err(RhiError::NoDevice { reason }) = concordia_rhi_metal::new_headless() {
        assert!(
            !gpu_required(),
            "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
        );
        return false;
    }
    true
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
fn capture_screenshot_defaults_to_an_agent_sized_768_by_432() {
    let Some(mut h) = Harness::gpu() else { return };
    h.cube_scene();
    assert_eq!(png_size(&h.ok("capture.screenshot", json!({}))), (768, 432));
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

#[test]
fn compiled_shaders_are_kept_in_the_project_ddc_for_the_next_start() {
    let Some(mut h) = Harness::gpu() else { return };
    h.cube_scene();
    h.ok("capture.screenshot", json!({"width": 32, "height": 32}));
    let ddc = h.dir.path().join(".concordia/ddc");
    let entries = walk(&ddc);
    assert!(
        entries >= 4,
        "shader modules cached in {}: {entries} files",
        ddc.display()
    );
}

fn walk(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir).map_or(0, |entries| {
        entries
            .flatten()
            .map(|e| {
                if e.path().is_dir() {
                    walk(&e.path())
                } else {
                    1
                }
            })
            .sum()
    })
}

#[test]
fn render_stats_reports_the_last_frame() {
    let Some(mut h) = Harness::gpu() else { return };
    assert_eq!(
        h.ok("render.stats", json!({}))["frame"],
        Value::Null,
        "nothing rendered yet"
    );
    h.cube_scene();
    h.ok(
        "capture.screenshot",
        json!({"width": 320, "height": 180, "view_mode": "wireframe"}),
    );
    // The GPU time arrives from the device's completion handler, just after the readback.
    let mut frame = Value::Null;
    for _ in 0..100 {
        frame = h.ok("render.stats", json!({}))["frame"].clone();
        if frame["gpu_ms"].is_number() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(frame["width"], 320, "{frame}");
    assert_eq!(frame["height"], 180);
    assert_eq!(frame["view_mode"], "wireframe");
    assert_eq!(frame["draws"], 1);
    assert_eq!(frame["culled"], 0);
    assert_eq!(frame["draw_calls"], 1);
    assert_eq!(frame["triangles"], 12, "a cube");
    assert!(frame["cpu_ms"].as_f64().unwrap() > 0.0, "{frame}");
    assert!(frame["gpu_ms"].as_f64().unwrap() > 0.0, "{frame}");
    assert!(frame["memory_bytes"].as_u64().unwrap() > 0, "{frame}");
}

/// An `Environment` on an entity without a `Transform` still lights a capture (agents add the
/// component to a bare entity).
#[test]
fn an_environment_without_a_transform_lights_captures() {
    let Some(mut h) = Harness::gpu() else { return };
    let hdr = "../../tests/fixtures/hdri/kloofendal_48d_partly_cloudy_puresky_512.hdr";
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(hdr),
        h.dir.path().join("sky.hdr"),
    )
    .unwrap();
    let map = h.ok("asset.import", json!({"path": "sky.hdr"}))["id"].clone();
    h.cube_scene();
    let dark = h.ok("capture.screenshot", json!({"width": 64, "height": 64}));
    h.ok("entity.spawn", json!({"actor": "agent:test", "components": {
        "Environment": {"map": format!("asset:{}", map.as_str().unwrap()), "intensity": 40000.0},
    }}));
    let lit = h.ok("capture.screenshot", json!({"width": 64, "height": 64}));
    assert_ne!(
        dark["image"], lit["image"],
        "the environment changed the image"
    );
    // The play world (a copy of the edit world) is lit the same way.
    h.ok("sim.step", json!({"ticks": 1}));
    let play = h.ok(
        "capture.screenshot",
        json!({"width": 64, "height": 64, "world": "play"}),
    );
    assert_eq!(
        play["image"], lit["image"],
        "the play world shows the environment too"
    );
    // And so is the scene reopened from its file.
    h.ok("sim.stop", json!({}));
    h.ok("scene.save", json!({}));
    h.ok("scene.open", json!({"path": "scenes/test.scene.ron"}));
    let reopened = h.ok("capture.screenshot", json!({"width": 64, "height": 64}));
    assert_eq!(
        reopened["image"], lit["image"],
        "a loaded scene shows its environment"
    );
}

/// Coat over paint: two closures.
const CAR_PAINT: &str = r#"(
    nodes: {
        "base": Slab((base_color: (0.6, 0.1, 0.05, 1.0), roughness: 0.5)),
        "coat": Slab((base_color: (0.0, 0.0, 0.0, 1.0), roughness: 0.15, transmittance: (1.0, 1.0, 1.0))),
        "paint": Vertical(top: "coat", bottom: "base"),
    },
    root: "paint",
)
"#;

impl Harness {
    /// Writes and imports a material file; returns its `asset:` reference.
    fn material(&mut self, path: &str, text: &str) -> String {
        std::fs::write(self.dir.path().join(path), text).unwrap();
        let id = self.ok("asset.import", json!({"path": path}))["id"].clone();
        format!("asset:{}", id.as_str().unwrap())
    }

    /// A sphere wearing `material` in front of a camera, lit by the sun; returns the sphere's id.
    fn sphere_scene(&mut self, material: &str) -> Value {
        let sphere = self.ok("entity.spawn", json!({"actor": "agent:test", "components": {
            "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.5, 1.5, 1.5]},
            "MeshRenderer": {"mesh": "builtin:mesh/sphere", "material": material},
        }}))["id"]
            .clone();
        self.ok("entity.spawn", json!({"actor": "agent:test", "components": {
            "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [-0.38, 0.2, 0.0, 0.9], "scale": [1.0, 1.0, 1.0]},
            "DirectionalLight": {},
        }}));
        self.ok("entity.spawn", json!({"actor": "agent:test", "components": {
            "Transform": {"translation": [0.0, 0.0, 3.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
            "Camera": {},
        }}));
        sphere
    }

    fn capture(&mut self) -> Value {
        self.ok("capture.screenshot", json!({"width": 96, "height": 96}))["image"].clone()
    }
}

fn decode_png(image: &Value) -> Vec<u8> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image["data_base64"].as_str().unwrap())
        .unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut rgba).unwrap();
    rgba
}

#[test]
fn closure_budget_downgrades_gracefully_and_reports_it() {
    if !has_gpu() {
        return;
    }
    // The render thread logs, so the ring must see every thread (nextest runs one test per process).
    let ring = Arc::new(LogRing::new(std::num::NonZeroUsize::new(256).unwrap()));
    tracing::subscriber::set_global_default(tracing_subscriber::layer::SubscriberExt::with(
        tracing_subscriber::registry(),
        ring.layer(),
    ))
    .unwrap();
    let budget = ClosureBudget::new(1).unwrap();
    let factory: DeviceFactory = Arc::new(concordia_rhi_metal::new_headless);
    let mut h = Harness::configured(
        RenderModule::new(factory.clone()).with_closure_budget(Some(budget)),
        Some(ring),
    );
    let paint = h.material("paint.material.ron", CAR_PAINT);
    let (fitted, _) = fit_budget(&read_material(CAR_PAINT).unwrap(), budget).unwrap();
    let fitted = h.material("fitted.material.ron", &write_material(&fitted).unwrap());
    let sphere = h.sphere_scene(&paint);
    let downgraded = h.capture();

    let frame = h.ok("render.stats", json!({}))["frame"].clone();
    assert_eq!(frame["closure_budget"], 1, "{frame}");
    assert_eq!(
        frame["downgraded_materials"],
        json!([{
            "material": paint,
            "before": 2,
            "after": 1,
            "merges": [{"node": "paint", "kind": "vertical", "slabs": ["coat", "base"], "dropped": []}],
        }])
    );
    let records = h.ok("log.tail", json!({"level": "info"}))["records"].clone();
    let logged = records
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["message"] == "material simplified to fit the closure budget")
        .unwrap_or_else(|| panic!("a log record: {records}"));
    assert_eq!(logged["fields"]["material"], paint);
    assert_eq!(logged["fields"]["closures"], "2 -> 1");

    // Gracefully: the sphere is still lit, exactly as the fitted material draws it.
    let rgba = decode_png(&downgraded);
    let center = &rgba[(48 * 96 + 48) * 4..][..3];
    assert!(center.iter().any(|c| *c > 40), "lit: {center:?}");
    h.ok(
        "component.patch",
        json!({"actor": "agent:test", "entity": sphere,
        "component": "MeshRenderer", "patch": {"material": fitted}}),
    );
    assert_eq!(
        h.capture(),
        downgraded,
        "the downgrade is the fitted material"
    );
    let frame = h.ok("render.stats", json!({}))["frame"].clone();
    assert_eq!(frame["downgraded_materials"], json!([]), "{frame}");

    // At the platform's budget the coat is drawn as authored.
    let mut full = Harness::with(factory);
    let paint = full.material("paint.material.ron", CAR_PAINT);
    full.sphere_scene(&paint);
    assert_ne!(
        full.capture(),
        downgraded,
        "the downgrade changed the image"
    );
    let frame = full.ok("render.stats", json!({}))["frame"].clone();
    assert_eq!(frame["closure_budget"], 8, "{frame}");
}

/// The dominant channel (0 red, 1 green, 2 blue) at the capture's center.
fn center_channel(image: &Value) -> usize {
    let rgba = decode_png(image);
    let center = &rgba[(48 * 96 + 48) * 4..][..3];
    (0..3).max_by_key(|i| center[*i]).unwrap()
}

#[test]
fn a_saved_material_redraws_with_its_new_content() {
    let Some(mut h) = Harness::gpu() else {
        return;
    };
    let path = "tint.material.ron";
    let tint = h.material(
        path,
        "(base_color: (0.8, 0.05, 0.05, 1.0), metallic: 0.0, roughness: 0.6)\n",
    );
    h.sphere_scene(&tint);
    assert_eq!(center_channel(&h.capture()), 0, "red at first");

    let mut material = h.ok("material.get", json!({"path": path}))["material"].clone();
    material["nodes"]["surface"]["Slab"]["base_color"] = json!([0.05, 0.05, 0.8, 1.0]);
    h.ok(
        "material.set",
        json!({"path": path, "material": material, "actor": "agent:test"}),
    );
    assert_eq!(center_channel(&h.capture()), 2, "material.set redraws");

    std::fs::write(
        h.dir.path().join(path),
        "(base_color: (0.05, 0.8, 0.05, 1.0), metallic: 0.0, roughness: 0.6)\n",
    )
    .unwrap();
    h.ok("asset.import", json!({"path": path}));
    assert_eq!(center_channel(&h.capture()), 1, "a re-import redraws too");
}
