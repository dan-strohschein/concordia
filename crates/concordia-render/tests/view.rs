//! The editor's own view: `capture.screenshot` with a `view` override and `viewport.pick`
//! (M1 phase 1.9). Picking is CPU-only; the capture cases skip without a GPU unless required.

use std::sync::Arc;

use concordia_api::{Engine, EngineConfig, EngineModule};
use concordia_assets::AssetsModule;
use concordia_core::id::SessionId;
use concordia_render::{RenderModule, View};
use concordia_rhi::RhiError;
use concordia_testkit::gpu_required;
use serde_json::{Value, json};

struct Harness {
    engine: Engine,
    session: SessionId,
    _dir: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        let modules: Vec<Arc<dyn EngineModule>> = vec![
            Arc::new(AssetsModule::new(dir.path())),
            Arc::new(RenderModule::new(Arc::new(
                concordia_rhi_metal::new_headless,
            ))),
        ];
        config.modules = modules;
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        engine
            .call(
                session,
                "scene.new",
                json!({"path": "scenes/v.scene.ron", "name": "V", "actor": "agent:test"}),
            )
            .unwrap();
        Self {
            engine,
            session,
            _dir: dir,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    /// Two unit cubes on the x axis: "Left" at x = -2 and "Right" at x = 2.
    fn two_cubes(&mut self) -> (Value, Value) {
        let mut cube = |name: &str, x: f32| {
            self.ok("entity.spawn", json!({"actor": "agent:test", "components": {
                "Name": name,
                "Transform": {"translation": [x, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
                "MeshRenderer": {"mesh": "builtin:mesh/cube", "material": "builtin:material/default"},
            }}))["id"]
                .clone()
        };
        (cube("Left", -2.0), cube("Right", 2.0))
    }

    fn gpu() -> Option<Self> {
        if let Err(RhiError::NoDevice { reason }) = concordia_rhi_metal::new_headless() {
            assert!(
                !gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            return None;
        }
        Some(Self::new())
    }
}

fn front_view() -> Value {
    json!({"eye": [0.0, 0.0, 10.0], "target": [0.0, 0.0, 0.0], "projection": {"Perspective": {"fov_y": 0.9, "near": 0.1, "far": 100.0}}})
}

#[test]
fn pick_returns_the_mesh_under_the_cursor() {
    let mut h = Harness::new();
    let (left, right) = h.two_cubes();
    let width = 400.0f32;
    let half_width = 10.0 * (0.45f32).tan(); // world half-height at z=0; square image
    let to_px = |x: f32| (width / 2.0 + x / half_width * width / 2.0) as u32;
    let hit = h.ok(
        "viewport.pick",
        json!({"view": front_view(), "x": to_px(2.0), "y": 200, "width": 400, "height": 400}),
    );
    assert_eq!(hit["entity"], right, "{hit}");
    assert!(
        (hit["distance"].as_f64().unwrap() - 9.5).abs() < 0.05,
        "front face is 9.5 away: {hit}"
    );
    let hit = h.ok(
        "viewport.pick",
        json!({"view": front_view(), "x": to_px(-2.0), "y": 200, "width": 400, "height": 400}),
    );
    assert_eq!(hit["entity"], left);
    let miss = h.ok(
        "viewport.pick",
        json!({"view": front_view(), "x": 200, "y": 200, "width": 400, "height": 400}),
    );
    assert!(
        miss["entity"].is_null(),
        "nothing between the cubes: {miss}"
    );
}

#[test]
fn pick_respects_rotation_and_scale() {
    let mut h = Harness::new();
    let (_, right) = h.two_cubes();
    // Top-down ortho, 20 px per unit: screen right is -X, screen up is +Z, so world (2, 0, -2.5) is pixel (160, 250).
    let top = json!({"eye": [0.0, 20.0, 0.0], "target": [0.0, 0.0, 0.0], "projection": {"Orthographic": {"height": 20.0, "near": 0.1, "far": 100.0}}});
    let at = |h: &mut Harness| {
        h.ok(
            "viewport.pick",
            json!({"view": top, "x": 160, "y": 250, "width": 400, "height": 400}),
        )
    };
    assert!(
        at(&mut h)["entity"].is_null(),
        "the unit cube at x = 2 does not reach z = -2.5"
    );
    // Stretch along local X by 6, then turn 90° about Y so the long axis runs along Z.
    h.ok("component.patch", json!({"entity": right, "component": "Transform", "actor": "agent:test", "patch": {"scale": [6.0, 1.0, 1.0], "rotation": [0.0, 0.70710677, 0.0, 0.70710677]}}));
    assert_eq!(
        at(&mut h)["entity"],
        right,
        "the rotated, stretched cube now covers it"
    );
}

#[test]
fn view_params_are_validated() {
    let mut h = Harness::new();
    let degenerate = json!({"eye": [0.0, 0.0, 0.0], "target": [0.0, 0.0, 0.0], "projection": {"Perspective": {"fov_y": 0.9, "near": 0.1, "far": 100.0}}});
    let err = h
        .call(
            "viewport.pick",
            json!({"view": degenerate, "x": 0, "y": 0, "width": 10, "height": 10}),
        )
        .unwrap_err();
    assert_eq!(err.data.kind, "InvalidParams", "{err:?}");
    let err = h
        .call(
            "viewport.pick",
            json!({"view": front_view(), "x": 50, "y": 0, "width": 10, "height": 10}),
        )
        .unwrap_err();
    assert_eq!(
        err.data.kind, "InvalidParams",
        "x outside the image: {err:?}"
    );
    let err = h
        .call(
            "capture.screenshot",
            json!({"view": front_view(), "camera": "0193a1c2-7e40-7000-8000-000000000001"}),
        )
        .unwrap_err();
    assert_eq!(
        err.data.kind, "InvalidParams",
        "camera and view are exclusive: {err:?}"
    );
}

#[test]
fn capture_with_a_view_renders_from_that_viewpoint_without_a_scene_camera() {
    let Some(mut h) = Harness::gpu() else { return };
    h.two_cubes();
    h.ok("entity.spawn", json!({"actor": "agent:test", "components": {
        "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [-0.38, 0.0, 0.0, 0.92], "scale": [1.0, 1.0, 1.0]},
        "DirectionalLight": {},
    }}));
    let toward = h.ok(
        "capture.screenshot",
        json!({"view": front_view(), "width": 64, "height": 64}),
    );
    let away = json!({"eye": [0.0, 0.0, 10.0], "target": [0.0, 0.0, 20.0], "projection": {"Perspective": {"fov_y": 0.9, "near": 0.1, "far": 100.0}}});
    let empty = h.ok(
        "capture.screenshot",
        json!({"view": away, "width": 64, "height": 64}),
    );
    assert_ne!(
        toward["image"], empty["image"],
        "looking at the cubes differs from looking away"
    );
}

#[test]
fn view_round_trips_as_a_reflected_type() {
    let view: View = serde_json::from_value(front_view()).unwrap();
    let again: View = serde_json::from_value(serde_json::to_value(&view).unwrap()).unwrap();
    assert_eq!(again, view, "f32 values survive serialization");
}

#[test]
fn viewport_present_needs_an_attached_surface() {
    let mut h = Harness::new();
    let err = h
        .call(
            "viewport.present",
            json!({"view": front_view(), "width": 64, "height": 64}),
        )
        .unwrap_err();
    assert_eq!(err.data.kind, "InvalidRequest", "{err:?}");
    assert!(
        err.data
            .suggestions
            .iter()
            .any(|s| s.contains("capture.screenshot")),
        "{err:?}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn viewport_present_draws_into_the_attached_layer() {
    let Some(mut h) = Harness::gpu() else { return };
    h.two_cubes();
    let layer = objc2_quartz_core::CAMetalLayer::new();
    let target = concordia_rhi::SurfaceTarget::MetalLayer(std::ptr::NonNull::from(&*layer).cast());
    // SAFETY: the layer outlives the engine (dropped at the end of the test, after `h`).
    unsafe { concordia_render::attach_surface(&mut h.engine, target) }.unwrap();
    let presented = (0..5)
        .filter(|_| {
            h.ok(
                "viewport.present",
                json!({"view": front_view(), "width": 64, "height": 48}),
            )["presented"]
                == true
        })
        .count();
    assert!(presented >= 4, "{presented}");
    drop(h);
}

#[cfg(target_os = "macos")]
#[test]
fn viewport_stats_record_each_presented_frame_with_its_input_tag() {
    let Some(mut h) = Harness::gpu() else { return };
    h.two_cubes();
    let layer = objc2_quartz_core::CAMetalLayer::new();
    let target = concordia_rhi::SurfaceTarget::MetalLayer(std::ptr::NonNull::from(&*layer).cast());
    // SAFETY: the layer outlives the engine (dropped after `h`).
    unsafe { concordia_render::attach_surface(&mut h.engine, target) }.unwrap();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        * 1000.0;
    // One frame at a time: frames queued faster than they render are coalesced (newest wins).
    for seq in 1..=3u64 {
        h.ok("viewport.present", json!({"view": front_view(), "width": 64, "height": 48, "seq": seq, "input_time_ms": now_ms}));
        wait_until_submitted(&mut h, seq);
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    let stats = h.ok("viewport.stats", json!({"clear": true}));
    let frames = stats["frames"].as_array().unwrap();
    assert_eq!(
        frames
            .iter()
            .map(|f| f["seq"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2, 3],
        "{stats}"
    );
    for f in frames {
        assert!(f["cpu_ms"].as_f64().unwrap() > 0.0, "{f}");
        assert!(
            f["gpu_ms"].as_f64().is_some_and(|ms| ms >= 0.0),
            "GPU time recorded: {f}"
        );
        assert!(f["submitted_ms"].as_f64().unwrap() >= now_ms, "{f}");
        assert_eq!(f["input_time_ms"].as_f64(), Some(now_ms));
    }
    assert!(
        h.ok("viewport.stats", json!({}))["frames"]
            .as_array()
            .unwrap()
            .is_empty(),
        "clear empties the record"
    );
    drop(h);
}

#[cfg(target_os = "macos")]
#[test]
fn queued_viewport_frames_coalesce_to_the_newest_view() {
    let Some(mut h) = Harness::gpu() else { return };
    h.two_cubes();
    let layer = objc2_quartz_core::CAMetalLayer::new();
    let target = concordia_rhi::SurfaceTarget::MetalLayer(std::ptr::NonNull::from(&*layer).cast());
    // SAFETY: the layer outlives the engine (dropped after `h`).
    unsafe { concordia_render::attach_surface(&mut h.engine, target) }.unwrap();
    h.ok(
        "viewport.present",
        json!({"view": front_view(), "width": 64, "height": 48}),
    );
    std::thread::sleep(std::time::Duration::from_millis(500));
    h.ok("viewport.stats", json!({"clear": true}));
    for seq in 1..=20u64 {
        h.ok(
            "viewport.present",
            json!({"view": front_view(), "width": 2048, "height": 2048, "seq": seq}),
        );
    }
    let mut seqs = Vec::new();
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        let stats = h.ok("viewport.stats", json!({}));
        seqs = stats["frames"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| !f["submitted_ms"].is_null())
            .map(|f| f["seq"].as_u64().unwrap())
            .collect();
        if seqs.last() == Some(&20) {
            break;
        }
    }
    assert_eq!(
        seqs.last(),
        Some(&20),
        "the newest view is always presented: {seqs:?}"
    );
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "in order: {seqs:?}");
    assert!(
        seqs.len() < 20,
        "stale views are skipped while the GPU is busy, not rendered on the engine thread: {seqs:?}"
    );
    drop(h);
}

/// Polls `viewport.stats` until frame `seq` has been rendered and submitted (up to 10 s).
#[cfg(target_os = "macos")]
fn wait_until_submitted(h: &mut Harness, seq: u64) {
    for _ in 0..200 {
        let stats = h.ok("viewport.stats", json!({}));
        let done = stats["frames"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["seq"] == seq && !f["submitted_ms"].is_null());
        if done {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("frame {seq} never rendered");
}

#[test]
fn viewport_navigate_moves_the_engine_side_editor_camera() {
    let mut h = Harness::new();
    let start = h.ok("viewport.navigate", json!({}))["view"].clone();
    let orbited = h.ok("viewport.navigate", json!({"orbit": [40.0, 0.0]}))["view"].clone();
    assert_ne!(start["eye"], orbited["eye"], "orbiting moves the eye");
    assert_eq!(start["target"], orbited["target"]);
    let top = h.ok("viewport.navigate", json!({"preset": "top"}))["view"].clone();
    assert!(top["eye"][1].as_f64().unwrap() > top["target"][1].as_f64().unwrap());
    let iso = h.ok("viewport.navigate", json!({"preset": "iso"}));
    assert!(
        iso["view"]["projection"]["Orthographic"].is_object(),
        "{iso}"
    );
    let focused = h.ok("viewport.navigate", json!({"focus": [3.0, 0.0, 0.0]}))["view"].clone();
    assert_eq!(focused["target"], json!([3.0, 0.0, 0.0]));
    let err = h
        .call("viewport.navigate", json!({"pan": [1.0, 1.0], "height": 0}))
        .unwrap_err();
    assert_eq!(err.kind, concordia_api::ApiErrorKind::InvalidParams);
}

#[cfg(target_os = "macos")]
#[test]
fn a_playing_viewport_redraws_on_each_simulation_tick_without_being_asked() {
    let Some(mut h) = Harness::gpu() else { return };
    h.two_cubes();
    let layer = objc2_quartz_core::CAMetalLayer::new();
    let target = concordia_rhi::SurfaceTarget::MetalLayer(std::ptr::NonNull::from(&*layer).cast());
    // SAFETY: the layer outlives the engine (dropped after `h`).
    unsafe { concordia_render::attach_surface(&mut h.engine, target) }.unwrap();
    h.ok("sim.play", json!({}));
    h.ok(
        "viewport.present",
        json!({"view": front_view(), "width": 64, "height": 48, "world": "play"}),
    );
    wait_until_rendered(&mut h, 1);
    h.ok("viewport.stats", json!({"clear": true}));
    let start = std::time::Instant::now();
    for i in 1..=5u64 {
        h.engine
            .poll(start + std::time::Duration::from_millis(20 * i));
    }
    wait_until_rendered(&mut h, 3);
    h.ok("sim.pause", json!({}));
}

/// Polls `viewport.stats` until at least `n` frames have rendered (up to 10 s).
#[cfg(target_os = "macos")]
fn wait_until_rendered(h: &mut Harness, n: usize) {
    for _ in 0..200 {
        let stats = h.ok("viewport.stats", json!({}));
        let rendered = stats["frames"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| !f["submitted_ms"].is_null())
            .count();
        if rendered >= n {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("fewer than {n} frames rendered");
}

fn translation_x(h: &mut Harness, id: &Value) -> f64 {
    h.ok(
        "component.get",
        json!({"entity": id, "component": "Transform"}),
    )["value"]["translation"][0]
        .as_f64()
        .unwrap()
}

#[test]
fn viewport_gizmo_drags_an_entity_along_an_axis_as_one_undo_step() {
    let mut h = Harness::new();
    let id = h.ok(
        "entity.spawn",
        json!({"components": {"Name": "Box", "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]}}, "actor": "human:local"}),
    )["id"]
        .clone();
    h.ok(
        "viewport.navigate",
        json!({"preset": "front", "focus": [0.0, 0.0, 0.0]}),
    );
    let at = |x: f64| json!({"x": x, "y": 150.0, "width": 400.0, "height": 300.0, "actor": "human:local"});
    let mut begin = at(200.0);
    begin["action"] = json!("begin");
    begin["entity"] = id.clone();
    begin["axis"] = json!("x");
    h.ok("viewport.gizmo", begin);
    for x in [230.0, 260.0] {
        let mut step = at(x);
        step["action"] = json!("move");
        h.ok("viewport.gizmo", step);
    }
    assert!(translation_x(&mut h, &id) > 0.5, "dragging right moves +x");
    h.ok(
        "viewport.gizmo",
        json!({"action": "end", "actor": "human:local"}),
    );
    h.ok("txn.undo", json!({"actor": "human:local"}));
    assert_eq!(
        translation_x(&mut h, &id),
        0.0,
        "the whole drag is one undo step"
    );
    let err = h
        .call(
            "viewport.gizmo",
            json!({"action": "move", "x": 1.0, "y": 1.0, "width": 10.0, "height": 10.0, "actor": "human:local"}),
        )
        .unwrap_err();
    assert_eq!(
        err.kind,
        concordia_api::ApiErrorKind::InvalidRequest,
        "no drag in progress"
    );
    let err = h.call("viewport.gizmo", json!({"action": "begin", "axis": "x", "x": 1.0, "y": 1.0, "width": 10.0, "height": 10.0, "actor": "human:local"})).unwrap_err();
    assert_eq!(
        err.kind,
        concordia_api::ApiErrorKind::InvalidParams,
        "begin needs an entity"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn camera_moves_through_the_viewport_control_skip_the_engine_thread() {
    let Some(mut h) = Harness::gpu() else { return };
    h.two_cubes();
    let layer = objc2_quartz_core::CAMetalLayer::new();
    let target = concordia_rhi::SurfaceTarget::MetalLayer(std::ptr::NonNull::from(&*layer).cast());
    // SAFETY: the layer outlives the engine (dropped after `h`).
    unsafe { concordia_render::attach_surface(&mut h.engine, target) }.unwrap();
    h.ok(
        "viewport.present",
        json!({"view": front_view(), "width": 64, "height": 48}),
    );
    wait_until_rendered(&mut h, 1);
    h.ok("viewport.stats", json!({"clear": true}));
    let control = concordia_render::viewport_control(&mut h.engine).unwrap();
    // The engine is neither polled nor called while the control works.
    let moved = std::thread::spawn(move || {
        control
            .navigate(&concordia_render::ViewportNavigateParams {
                orbit: Some([25.0, 0.0]),
                seq: Some(77),
                ..Default::default()
            })
            .unwrap()
    })
    .join()
    .unwrap();
    wait_until_rendered(&mut h, 1);
    let stats = h.ok("viewport.stats", json!({}));
    assert_eq!(stats["frames"][0]["seq"], 77, "{stats}");
    let engine_view = h.ok("viewport.navigate", json!({}))["view"].clone();
    assert_eq!(
        engine_view,
        serde_json::to_value(moved).unwrap(),
        "one camera"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn an_idle_viewport_renders_nothing() {
    let Some(mut h) = Harness::gpu() else { return };
    h.two_cubes();
    let layer = objc2_quartz_core::CAMetalLayer::new();
    let target = concordia_rhi::SurfaceTarget::MetalLayer(std::ptr::NonNull::from(&*layer).cast());
    // SAFETY: the layer outlives the engine (dropped after `h`).
    unsafe { concordia_render::attach_surface(&mut h.engine, target) }.unwrap();
    h.ok(
        "viewport.present",
        json!({"view": front_view(), "width": 64, "height": 48}),
    );
    wait_until_rendered(&mut h, 1);
    h.ok("viewport.stats", json!({"clear": true}));
    std::thread::sleep(std::time::Duration::from_millis(500));
    let stats = h.ok("viewport.stats", json!({}));
    assert_eq!(stats["frames"], json!([]), "no work while nothing changes");
}
