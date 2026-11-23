//! A gameplay module reads player input (M2 phase 2.10a, gap S1): `inputg.walk` reads the
//! engine's `InputActions` and drives a character, as WASM and as a native library.
//!
//! D (`camera_pan` right, (1, 0)) is held from tick 0 and Space (`spawn_box`) pressed on tick 0.
//! `input.actions` mirrors them in `PrePhysics` before `inputg.walk`, which sets the motion to
//! (1, 0, −0) × 2 = (2, 0, 0) m/s the same tick, so physics moves the walker from tick 0: after
//! 30 ticks it is 30 × 2 / 60 = 1 m along x. Space stays held, so presses stay 1 (only
//! `just_pressed` counts). The same run gives the same world hash in both builds.

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";
const MAP: &str = include_str!("../../../examples/m1-physics-stack/input/default.input.ron");

struct Stage {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: concordia_core::id::SessionId,
}

impl Stage {
    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
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

    fn play(&mut self, entity: &str, component: &str) -> Value {
        self.ok(
            "component.get",
            json!({"entity": entity, "component": component, "world": "play"}),
        )["value"]
            .clone()
    }
}

#[derive(Clone, Copy)]
enum Build {
    Wasm,
    Native,
}

fn stage(build: Build) -> Stage {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("scripts")).unwrap();
    std::fs::create_dir_all(root.join("input")).unwrap();
    std::fs::write(root.join("input/default.input.ron"), MAP).unwrap();
    let module = match build {
        Build::Wasm => {
            let guest =
                concordia_tests::wasm_fixture_path("input_guest").unwrap_or_else(|e| panic!("{e}"));
            std::fs::copy(guest, root.join("scripts/input_guest.wasm")).unwrap();
            "kind = \"wasm\"\npath = \"scripts/input_guest.wasm\"\n"
        }
        Build::Native => {
            let guest = concordia_tests::native_fixture_path("input_guest")
                .unwrap_or_else(|e| panic!("{e}"));
            let rel = format!("scripts/input_guest.{}", std::env::consts::DLL_EXTENSION);
            std::fs::copy(guest, root.join(rel)).unwrap();
            "kind = \"native\"\npath = \"scripts/input_guest\"\n"
        }
    };
    let manifest = format!(
        "[input]\nmap = \"input/default.input.ron\"\n\n[physics]\n\n[scripts]\n[[scripts.module]]\n{module}"
    );
    std::fs::write(root.join("concordia.toml"), manifest).unwrap();
    let project = Project::load(root).unwrap();
    let mut config = EngineConfig::new(root);
    config.deterministic_ids = Some(11);
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let session = engine.open_session();
    let mut stage = Stage {
        _dir: dir,
        engine,
        session,
    };
    stage.ok(
        "scene.new",
        json!({"path": "scenes/a.scene.ron", "name": "A", "actor": ACTOR}),
    );
    stage
}

fn at(x: f32, y: f32, z: f32) -> Value {
    json!({"translation": [x, y, z], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]})
}

/// Runs the scenario and returns the final world hash.
fn walk(build: Build) -> String {
    let mut stage = stage(build);
    stage.spawn(json!({
        "Transform": at(0.0, -0.5, 0.0),
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [20.0, 0.5, 20.0]}}},
    }));
    let walker = stage.spawn(json!({
        "Transform": at(0.0, 0.0, 0.0),
        "CharacterController": {},
        "CharacterMotion": {},
        "InputActions": {"watch": ["camera_pan", "spawn_box"]},
        "inputg.Walker": {"speed": 2.0, "presses": 0},
    }));
    let key = |key: &str| json!({"tick": 0, "event": {"Key": {"key": key, "pressed": true}}});
    stage.ok(
        "input.inject",
        json!({"events": [key("D"), key("Space")], "actor": ACTOR}),
    );

    stage.ok("sim.step", json!({"ticks": 1}));
    let motion = stage.play(&walker, "CharacterMotion");
    assert_eq!(motion["velocity"], json!([2.0, 0.0, 0.0]), "{motion}");
    let hash = stage.ok("sim.step", json!({"ticks": 29}))["world_hash"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(stage.play(&walker, "inputg.Walker")["presses"], 1);
    let x = stage.play(&walker, "Transform")["translation"][0]
        .as_f64()
        .unwrap();
    assert!((x - 1.0).abs() < 0.01, "x = {x}");
    hash
}

#[test]
fn wasm_guest_reads_input_actions() {
    walk(Build::Wasm);
}

#[test]
fn native_guest_reads_input_actions_with_the_same_hash() {
    assert_eq!(walk(Build::Native), walk(Build::Wasm));
}
