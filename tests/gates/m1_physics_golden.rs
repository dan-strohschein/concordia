//! M1 gate: the physics stack scene simulates, settles and renders the same way every time, all
//! through the in-process Engine API. Image checks need a GPU (required with
//! `CONCORDIA_REQUIRE_GPU=1`); the physics checks run everywhere.

use std::collections::BTreeMap;

use base64::Engine as _;
use concordia_api::{ApiErrorKind, Engine, EngineConfig};
use concordia_core::id::SessionId;
use concordia_core::math::{Quat, Vec3};
use concordia_render::{Projection, View};
use concordia_testkit::golden::{RgbaImage, assert_golden};
use concordia_testkit::gpu_required;
use serde_json::{Value, json};

const TICKS: u64 = 180;
const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const DROPPED: [&str; 3] = ["Dropped 0", "Dropped 1", "Dropped 2"];

/// An engine on a fresh copy of the example, with the gate scene open, as `agent:gate`.
struct Gate {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
    ids: BTreeMap<String, String>,
}

impl Gate {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        concordia_tests::copy_example("m1-physics-stack", dir.path()).unwrap();
        let modules = concordia_app::Project::load(dir.path())
            .and_then(|p| p.modules())
            .expect("the example's modules load");
        let mut config = EngineConfig::new(dir.path());
        config.modules = modules;
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut gate = Self {
            _dir: dir,
            engine,
            session,
            ids: BTreeMap::new(),
        };
        gate.ok(
            "session.open",
            json!({"client": "m1-gate", "actor": "agent:gate"}),
        );
        gate.ok("scene.open", json!({"path": "scenes/stack.scene.ron"}));
        let rows = gate.ok("query.run", json!({"with": ["Name"], "select": ["Name"]}));
        gate.ids = rows["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let name = r["components"]["Name"].as_str().unwrap().to_owned();
                (name, r["id"].as_str().unwrap().to_owned())
            })
            .collect();
        gate
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    fn id(&self, name: &str) -> &str {
        self.ids
            .get(name)
            .unwrap_or_else(|| panic!("no entity named {name:?}"))
    }

    fn component(&mut self, name: &str, component: &str, world: &str) -> Value {
        let entity = self.id(name).to_owned();
        self.ok(
            "component.get",
            json!({"entity": entity, "component": component, "world": world}),
        )["value"]
            .clone()
    }

    /// Steps the simulation and asserts physics really ran; returns `sim.step`'s world hash.
    fn settle(&mut self) -> String {
        let start: Vec<f64> = DROPPED
            .iter()
            .map(|n| y_of(&self.component(n, "Transform", "edit")))
            .collect();
        let step = self.ok("sim.step", json!({"ticks": TICKS, "actor": "agent:gate"}));
        for (name, start) in DROPPED.iter().zip(start) {
            let end = y_of(&self.component(name, "Transform", "play"));
            assert!(end < start - 0.5, "{name} fell: {start} → {end}");
        }
        let bodies = self.ok(
            "query.run",
            json!({"with": ["RigidBody"], "select": ["RigidBody"], "world": "play"}),
        );
        for row in bodies["rows"].as_array().unwrap() {
            if row["components"]["RigidBody"]["kind"] != "Dynamic" {
                continue;
            }
            let entity = row["id"].clone();
            let velocity = self.ok(
                "component.get",
                json!({"entity": entity, "component": "Velocity", "world": "play"}),
            )["value"]["linear"]
                .clone();
            let speed = length(&velocity);
            assert!(speed < 0.05, "{entity} settled: {speed} m/s");
        }
        step["world_hash"].as_str().unwrap().to_owned()
    }

    /// The play world from `gate_cam` as PNG bytes; `None` without a GPU (unless one is required).
    fn capture(&mut self) -> Option<Vec<u8>> {
        let camera = self.id("gate_cam").to_owned();
        let params = json!({"camera": camera, "width": WIDTH, "height": HEIGHT, "world": "play"});
        match self.engine.call(self.session, "capture.screenshot", params) {
            Ok(shot) => Some(
                base64::engine::general_purpose::STANDARD
                    .decode(shot["image"]["data_base64"].as_str().unwrap())
                    .unwrap(),
            ),
            Err(e) if e.kind == ApiErrorKind::GpuUnavailable && !gpu_required() => None,
            Err(e) => panic!("capture.screenshot: {e:?}"),
        }
    }

    /// `gate_cam`'s view, to find where world points land in the capture.
    fn gate_view(&mut self) -> View {
        let t = self.component("gate_cam", "Transform", "edit");
        let eye = vec3(&t["translation"]);
        let r = &t["rotation"];
        let rotation = Quat::from_xyzw(f(&r[0]), f(&r[1]), f(&r[2]), f(&r[3]));
        let camera = self.component("gate_cam", "Camera", "edit");
        let projection: Projection = serde_json::from_value(camera["projection"].clone()).unwrap();
        View {
            eye,
            target: eye + rotation * Vec3::NEG_Z,
            projection,
        }
    }
}

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap() as f32
}

fn vec3(v: &Value) -> Vec3 {
    Vec3::new(f(&v[0]), f(&v[1]), f(&v[2]))
}

fn y_of(transform: &Value) -> f64 {
    transform["translation"][1].as_f64().unwrap()
}

fn length(v: &Value) -> f64 {
    (0..3)
        .map(|i| v[i].as_f64().unwrap().powi(2))
        .sum::<f64>()
        .sqrt()
}

fn pixel(image: &RgbaImage, [x, y]: [f32; 2]) -> [f32; 3] {
    let i = ((y as u32 * image.width() + x as u32) * 4) as usize;
    let p = &image.pixels()[i..i + 3];
    [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])]
}

/// Mean luminance over a 5×5 block around `at`.
fn luminance(image: &RgbaImage, at: [f32; 2]) -> f32 {
    let mut sum = 0.0;
    for dy in -2..=2 {
        for dx in -2..=2 {
            let p = pixel(image, [at[0] + dx as f32, at[1] + dy as f32]);
            sum += 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
        }
    }
    sum / 25.0
}

/// Fixed regions of the 1280×720 gate image (the scene is deterministic and its state hash is
/// snapshotted): inside the pyramid's shadow, and sunlit ground beside it.
const PYRAMID_SHADOW: [f32; 2] = [790.0, 413.0];
const SUNLIT_GROUND: [f32; 2] = [840.0, 430.0];

/// Guards against blessing a broken frame: the pyramid's shadow is at least 30% darker than sunlit
/// ground beside it, and the ground under three ring lights shows each light's hue.
fn assert_not_degenerate(gate: &mut Gate, image: &RgbaImage) {
    let (dark, lit) = (
        luminance(image, PYRAMID_SHADOW),
        luminance(image, SUNLIT_GROUND),
    );
    assert!(dark < lit * 0.7, "shadow {dark} vs sunlit ground {lit}");
    let view = gate.gate_view();
    let project = |p: Vec3| {
        view.project(p, WIDTH as f32, HEIGHT as f32)
            .expect("in view")
    };
    for (light, channel) in [
        ("Ring light 0", 0),
        ("Ring light 5", 1),
        ("Ring light 11", 2),
    ] {
        let at = vec3(&gate.component(light, "Transform", "edit")["translation"])
            * Vec3::new(1.0, 0.0, 1.0);
        let [x, y] = project(at);
        let tinted = (-8..=8)
            .flat_map(|dy| (-8..=8).map(move |dx| [x + dx as f32, y + dy as f32]))
            .any(|p| {
                let c = pixel(image, p);
                (0..3)
                    .filter(|&k| k != channel)
                    .all(|k| c[channel] >= c[k] + 6.0)
            });
        assert!(tinted, "{light}'s hue shows near {:?}", [x, y]);
    }
}

#[test]
fn m1_gate_physics_scene_golden() {
    let mut gate = Gate::new();
    let hash = gate.settle();
    insta::assert_snapshot!("m1_gate_state_hash", hash);
    let Some(png) = gate.capture() else { return };
    let image = RgbaImage::from_png(&png).unwrap();
    assert_eq!((image.width(), image.height()), (WIDTH, HEIGHT));
    assert_golden("gates", "m1_physics_stack", &image);
    assert_not_degenerate(&mut gate, &image);

    let mut again = Gate::new();
    assert_eq!(again.settle(), hash, "physics is deterministic");
    assert_eq!(
        again.capture().unwrap(),
        png,
        "and so is the image, byte for byte"
    );
}

#[test]
fn m1_gate_state_hash_cross_platform() {
    let mut gate = Gate::new();
    let hash = gate.settle();
    insta::assert_snapshot!("m1_gate_state_hash", hash);
}
