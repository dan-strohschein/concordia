//! `replay.run` in `ab` mode on project engines (M3 phase 3.1c): the app gives the replay module
//! a factory that builds the other project's engine from its own manifest. A copy of the
//! project is the same build, so a 30-tick replay agrees for all 30 ticks.

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use concordia_input::InputMap;
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";
const MANIFEST: &str = "[input]\nmap = \"input/game.input.ron\"\n\n[replay]\n";
const PATH: &str = "tests/replays/a.replay.ron";

fn ok(engine: &mut Engine, s: concordia_core::id::SessionId, method: &str, params: Value) -> Value {
    engine
        .call(s, method, params.clone())
        .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
}

#[test]
fn ab_runs_against_another_project_through_the_apps_factory() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), MANIFEST).unwrap();
    std::fs::create_dir_all(dir.path().join("input")).unwrap();
    std::fs::write(
        dir.path().join("input/game.input.ron"),
        InputMap::starter().to_ron().unwrap(),
    )
    .unwrap();
    let project = Project::load(dir.path()).unwrap();
    let mut config = EngineConfig::new(dir.path());
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    ok(
        &mut engine,
        s,
        "scene.new",
        json!({"path": "scenes/a.scene.ron", "name": "A", "actor": ACTOR}),
    );
    ok(
        &mut engine,
        s,
        "entity.spawn",
        json!({"components": {"Name": "Thing"}, "actor": ACTOR}),
    );
    ok(&mut engine, s, "scene.save", json!({"actor": ACTOR}));
    ok(
        &mut engine,
        s,
        "replay.record",
        json!({"path": PATH, "actor": ACTOR}),
    );
    ok(&mut engine, s, "sim.step", json!({"ticks": 30}));
    ok(&mut engine, s, "replay.record", json!({"action": "stop"}));
    let other = tempfile::tempdir().unwrap();
    concordia_tests::copy_project(dir.path(), other.path()).unwrap();
    let run = ok(
        &mut engine,
        s,
        "replay.run",
        json!({"path": PATH, "mode": "ab", "other": other.path().to_str().unwrap()}),
    );
    assert_eq!(
        (&run["ok"], &run["ticks"]),
        (&json!(true), &json!(30)),
        "{run}"
    );
}
