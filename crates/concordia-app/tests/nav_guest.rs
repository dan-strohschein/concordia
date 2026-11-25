//! A WASM guest drives navigation and hears engine signals (M2 phase 2.5b.1, the carried-over
//! `AnimEvent` item): `navg.go` sets a `NavAgent`'s target by writing the component, and
//! `navg.listen` receives `AnimEvent` (a clip event on an imported character) and `NavArrived`.
//!
//! The walker goes from (−5, 0) to (5.23, 0) at 3 m/s on a floor walkable to ±9.5. The guest
//! writes the target in tick 0's `PostPhysics`, so `nav.plan` sees it on tick 1 and the walk
//! starts there: one tick later than `concordia-nav`'s `agents_physics.rs`, it arrives on tick
//! 196 at x = 4.75 and the guest hears it on tick 197. Walk's "step" event is at 0.25 s.

use std::path::Path;

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";

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

    fn import(&mut self, path: &str) -> String {
        self.ok("asset.import", json!({"path": path, "actor": ACTOR}))["id"]
            .as_str()
            .unwrap()
            .to_owned()
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

    fn heard(&mut self, entity: &str) -> Value {
        self.ok(
            "component.get",
            json!({"entity": entity, "component": "navg.Heard", "world": "play"}),
        )["value"]
            .clone()
    }
}

const MANIFEST: &str = "[assets]\n\n[anim]\n\n[physics]\n\n[nav]\n\n[scripts]\n[[scripts.module]]\nkind = \"wasm\"\npath = \"scripts/nav_guest.wasm\"\n";

fn stage() -> Stage {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("concordia.toml"), MANIFEST).unwrap();
    let fbx = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fbx");
    std::fs::copy(fbx.join("skinned.fbx"), root.join("hero.fbx")).unwrap();
    std::fs::copy(fbx.join("anim_stacks.fbx"), root.join("moves.fbx")).unwrap();
    let guest = concordia_tests::wasm_fixture_path("nav_guest").unwrap_or_else(|e| panic!("{e}"));
    std::fs::create_dir_all(root.join("scripts")).unwrap();
    std::fs::copy(guest, root.join("scripts/nav_guest.wasm")).unwrap();
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

fn nothing_heard() -> Value {
    json!({"anim": [], "arrived": 0, "arrived_at": [0.0, 0.0, 0.0]})
}

#[test]
fn guest_receives_anim_and_nav_signals() {
    let mut stage = stage();
    stage.import("moves.fbx");
    let sidecar = stage._dir.path().join("moves.fbx.import.ron");
    let text = std::fs::read_to_string(&sidecar).unwrap();
    let events =
        r#"settings: {"clip_events": {"Armature|Walk": [{"time": 0.25, "name": "step"}]}}"#;
    std::fs::write(&sidecar, text.replace(NEW_MODEL_SETTINGS, events)).unwrap();
    let moves = stage.import("moves.fbx");
    let hero = stage.import("hero.fbx");
    let made = stage.ok(
        "prefab.instantiate",
        json!({"asset": format!("asset:{hero}"), "actor": ACTOR}),
    );
    let body = made["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_owned())
        .find(|e| {
            stage.ok("entity.get", json!({"id": e}))["components"]
                .get("Skinned")
                .is_some()
        })
        .expect("a skinned body");
    let walk = format!("asset:{moves}#clip/Armature|Walk");
    for (component, value) in [
        ("AnimPlayer", json!({"clip": walk})),
        ("navg.Heard", nothing_heard()),
    ] {
        stage.ok(
            "component.set",
            json!({"entity": body, "component": component, "value": value, "actor": ACTOR}),
        );
    }
    stage.spawn(json!({
        "Transform": at(0.0, -0.5, 0.0),
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [10.1, 0.5, 10.1]}}},
    }));
    stage.spawn(json!({
        "Transform": at(0.0, 0.0, 0.0),
        "NavMeshVolume": {"half_extents": [12.0, 3.0, 12.0]},
    }));
    let walker = stage.spawn(json!({
        "Transform": at(-5.0, 0.0, 0.0),
        "CharacterController": {},
        "CharacterMotion": {},
        "NavAgent": {"max_speed": 3.0},
        "navg.Goal": {"at": [5.23, 0.0, 0.0]},
        "navg.Heard": nothing_heard(),
    }));

    stage.ok("sim.step", json!({"ticks": 1}));
    let agent = stage.ok(
        "component.get",
        json!({"entity": walker, "component": "NavAgent", "world": "play"}),
    );
    let target = &agent["value"]["target"];
    assert!(
        (target[0].as_f64().unwrap() - 5.23).abs() < 1e-6,
        "the guest set the target: {agent}"
    );
    stage.ok("sim.step", json!({"ticks": 196})); // ticks 1..=196
    assert_eq!(
        stage.heard(&walker)["arrived"],
        0,
        "raised on tick 196, heard next tick"
    );
    stage.ok("sim.step", json!({"ticks": 1})); // tick 197
    let heard = stage.heard(&walker);
    assert_eq!(heard["arrived"], 1, "{heard}");
    let x = heard["arrived_at"][0].as_f64().unwrap();
    assert!((x - 4.75).abs() < 1e-3, "{heard}");
    let anim = stage.heard(&body)["anim"].clone();
    let first = anim[0]
        .as_str()
        .unwrap_or_else(|| panic!("no AnimEvent heard: {anim}"));
    assert_eq!(first, format!("step@0.25 in {walk}"), "{anim}");
}

/// A new model sidecar's settings block (the model settings left to the project, ADR-0102).
const NEW_MODEL_SETTINGS: &str = "settings: {\n        \"emissive_reference_ev100\": None,\n        \"meshlets\": \"auto\",\n    }";
