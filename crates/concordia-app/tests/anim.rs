//! `[anim]`: an imported character plays a clip in the play world through the Engine API (M2 2.4.1).

use std::path::Path;

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";

#[test]
fn anim_module_plays_an_imported_clip_through_sim_step() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("concordia.toml"), "[assets]\n\n[anim]\n").unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fbx");
    for (from, to) in [
        ("skinned.fbx", "hero.fbx"),
        ("anim_stacks.fbx", "moves.fbx"),
    ] {
        std::fs::copy(fixtures.join(from), root.join(to)).unwrap();
    }
    let project = Project::load(root).unwrap();
    let mut config = EngineConfig::new(root);
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    let mut ok = |method: &str, params: Value| {
        engine
            .call(s, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    };
    ok(
        "scene.new",
        json!({"path": "scenes/a.scene.ron", "name": "A", "actor": ACTOR}),
    );
    let mut import = |path: &str| {
        ok("asset.import", json!({"path": path, "actor": ACTOR}))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let hero = import("hero.fbx");
    let moves = import("moves.fbx");
    let made = ok(
        "prefab.instantiate",
        json!({"asset": format!("asset:{hero}"), "actor": ACTOR}),
    );
    let body = made["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            ok("entity.get", json!({"id": e}))["components"]
                .get("Skinned")
                .is_some()
        })
        .expect("the instance has a skinned body")
        .clone();
    ok(
        "component.set",
        json!({
            "entity": body,
            "component": "AnimPlayer",
            "value": {"clip": format!("asset:{moves}#clip/Armature|Walk")},
            "actor": ACTOR,
        }),
    );
    ok("sim.step", json!({"ticks": 30}));
    let get = |ok: &mut dyn FnMut(&str, Value) -> Value, c: &str| {
        ok(
            "component.get",
            json!({"entity": body, "component": c, "world": "play"}),
        )["value"]
            .clone()
    };
    let player = get(&mut ok, "AnimPlayer");
    let t = player["time"].as_f64().unwrap();
    assert!((t - 0.5).abs() < 1e-4, "30 ticks at 60 Hz: {player}");
    let pose = get(&mut ok, "Pose");
    assert_eq!(pose["bones"].as_array().map(Vec::len), Some(7), "{pose}");
    ok("sim.step", json!({"ticks": 7}));
    assert_ne!(get(&mut ok, "Pose"), pose, "the pose follows the clip");
    let edit = ok("entity.get", json!({"id": body}));
    assert!(
        edit["components"].get("Pose").is_none(),
        "play never writes the edit world"
    );
}
