//! Click-to-move in a project (M2 phase 2.5b.2a, ADR-0078), the real picking path: a marker is
//! projected to a pixel of the game camera's image, `viewport.pointer_ray` turns the pixel back into
//! a world ray, `input.inject` sends the ray and a click, and the hero walks there.
//!
//! The camera looks from (0, 14, 12) at the origin; the marker is at (4, 0, −3) on a floor walkable
//! to ±9.5. The ray through the marker's pixel hits the floor's top within a fraction of a
//! millimetre of the marker (pixel coordinates are fractional, so the round trip is exact up to
//! float error), the hit snaps up to (4, 0.2, −3), and the hero arrives within its 0.5 m arrival
//! distance. The same session gives the same hashes at 1 and 8 threads.

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use concordia_core::math::Vec3;
use concordia_render::{Projection, View};
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";
const MAP: &str = r#"InputMap(
    actions: {
        "move": (kind: Button, bindings: [MouseButton(Left)]),
        "pointer_ray": (kind: Ray, bindings: [PointerRay]),
    },
)"#;
const MANIFEST: &str =
    "[input]\nmap = \"input/game.input.ron\"\n\n[render]\n\n[physics]\n\n[nav]\n";
const MARKER: Vec3 = Vec3::new(4.0, 0.0, -3.0);
const SIZE: (u32, u32) = (960, 540);

struct Stage {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: concordia_core::id::SessionId,
}

impl Stage {
    fn new(threads: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("concordia.toml"), MANIFEST).unwrap();
        std::fs::create_dir_all(dir.path().join("input")).unwrap();
        std::fs::write(dir.path().join("input/game.input.ron"), MAP).unwrap();
        let project = Project::load(dir.path()).unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(13);
        config.threads = std::num::NonZeroUsize::new(threads).unwrap();
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut stage = Self {
            _dir: dir,
            engine,
            session,
        };
        stage.ok(
            "scene.new",
            json!({"path": "scenes/c.scene.ron", "name": "C", "actor": ACTOR}),
        );
        stage
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    fn spawn(&mut self, components: Value) -> String {
        self.ok(
            "entity.spawn",
            json!({"components": components, "actor": ACTOR}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn play(&mut self, id: &str, component: &str) -> Value {
        self.ok(
            "component.get",
            json!({"entity": id, "component": component, "world": "play"}),
        )["value"]
            .clone()
    }

    fn step(&mut self, ticks: u32) -> String {
        self.ok("sim.step", json!({"ticks": ticks}))["world_hash"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

fn at(p: [f32; 3], rotation: [f32; 4]) -> Value {
    json!({"translation": p, "rotation": rotation, "scale": [1.0, 1.0, 1.0]})
}

fn camera_view() -> View {
    View {
        eye: Vec3::new(0.0, 14.0, 12.0),
        target: Vec3::ZERO,
        projection: Projection::Perspective {
            fov_y: 0.8,
            near: 0.1,
            far: 100.0,
        },
    }
}

/// Builds the level, clicks the marker's pixel and walks; returns the hashes and where the hero
/// ended.
fn click_and_walk(threads: usize) -> (Vec<String>, Value, Value) {
    let mut stage = Stage::new(threads);
    stage.spawn(json!({
        "Transform": at([0.0, -0.5, 0.0], [0.0, 0.0, 0.0, 1.0]),
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [10.1, 0.5, 10.1]}}},
    }));
    stage.spawn(json!({
        "Transform": at([0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
        "NavMeshVolume": {"half_extents": [12.0, 3.0, 12.0]},
    }));
    let view = camera_view();
    let (_, rotation, eye) = view.world_matrix().unwrap().to_scale_rotation_translation();
    stage.spawn(json!({
        "Transform": at(eye.to_array(), rotation.to_array()),
        "Camera": {"projection": {"Perspective": {"fov_y": 0.8, "near": 0.1, "far": 100.0}}},
    }));
    let hero = stage.spawn(json!({
        "Transform": at([-5.0, 0.0, 4.0], [0.0, 0.0, 0.0, 1.0]),
        "CharacterController": {},
        "CharacterMotion": {},
        "NavAgent": {"max_speed": 4.0},
        "ClickToMove": {},
    }));
    let mut hashes = vec![stage.step(1)];
    let [x, y] = view
        .project(MARKER, SIZE.0 as f32, SIZE.1 as f32)
        .expect("in view");
    let ray = stage.ok(
        "viewport.pointer_ray",
        json!({"x": x, "y": y, "width": SIZE.0, "height": SIZE.1, "world": "play"}),
    );
    stage.ok(
        "input.inject",
        json!({"events": [
            {"event": {"PointerRay": {"origin": ray["origin"], "direction": ray["direction"]}}},
            {"event": {"MouseButton": {"button": "Left", "pressed": true}}},
        ], "actor": ACTOR}),
    );
    hashes.push(stage.step(1));
    let target = stage.play(&hero, "NavAgent")["target"].clone();
    hashes.push(stage.step(240));
    let state = stage.play(&hero, "NavAgentState");
    assert_eq!(state["status"], "Arrived", "{state}");
    let feet = stage.play(&hero, "Transform")["translation"].clone();
    (hashes, target, feet)
}

#[test]
fn player_click_to_move_reaches_marker() {
    let (one, target, feet) = click_and_walk(1);
    let t: Vec<f64> = (0..3).map(|i| target[i].as_f64().unwrap()).collect();
    assert!(
        (t[0] - 4.0).abs() < 1e-3 && (t[1] - 0.2).abs() < 1e-3 && (t[2] + 3.0).abs() < 1e-3,
        "the marker's pixel came back as the marker: {target}"
    );
    let (fx, fz) = (feet[0].as_f64().unwrap(), feet[2].as_f64().unwrap());
    assert!(
        ((fx - 4.0).powi(2) + (fz + 3.0).powi(2)).sqrt() <= 0.5,
        "{feet}"
    );
    let (eight, _, _) = click_and_walk(8);
    assert_eq!(one, eight, "1 vs 8 threads");
}
