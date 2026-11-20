//! Replays and script modules: fingerprints in the header, hot reload while recording (2.8).

mod common;

use std::path::Path;
use std::sync::Arc;

use common::{Opts, Rig};
use concordia_api::ApiErrorKind;
use concordia_replay::read_replay;
use concordia_scripting::{ScriptEntry, ScriptKind, ScriptsConfig, ScriptsModule};
use serde_json::json;

const PATH: &str = "tests/replays/scripted.replay.ron";
const WASM: &str = "scripts/conformance.wasm";

fn place(dir: &Path, fixture: &str) {
    let from = concordia_tests::wasm_fixture_path(fixture).unwrap_or_else(|e| panic!("{e}"));
    let to = dir.join(WASM);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::copy(from, to).unwrap();
}

/// A rig whose project runs the conformance module, with one scripted mover in the scene.
fn scripted() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    place(dir.path(), "conformance_guest");
    let mut config = ScriptsConfig::for_project(dir.path(), 5);
    config.modules = vec![ScriptEntry {
        kind: ScriptKind::Wasm,
        path: dir.path().join(WASM),
        source: None,
    }];
    let mut rig = Rig::in_dir(
        dir,
        Opts {
            extra: vec![Arc::new(ScriptsModule::new(config))],
            ..Opts::default()
        },
    );
    rig.ok(
        "entity.spawn",
        json!({"components": {
            "conf.Position": {"x": 1.0, "y": 2.0},
            "conf.Velocity": {"x": 1.0, "y": 3.0},
        }}),
    );
    rig
}

fn checkpoints(rig: &Rig) -> Vec<String> {
    let replay = read_replay(&rig.file(PATH)).unwrap();
    replay.checkpoints.into_iter().map(|c| c.world).collect()
}

#[test]
fn replay_header_names_the_loaded_script_builds() {
    let mut rig = scripted();
    rig.record(PATH, 60, json!([]), 60);
    let replay = read_replay(&rig.file(PATH)).unwrap();
    let scripts: Vec<(&str, &str)> = replay
        .header
        .modules
        .iter()
        .filter(|m| m.module == "scripts")
        .map(|m| (m.name.as_str(), m.kind.as_str()))
        .collect();
    assert_eq!(scripts, [("conformance", "wasm")]);
}

#[test]
fn hot_reload_of_identical_module_mid_recording_keeps_hash() {
    let mut rig = scripted();
    rig.record(PATH, 30, json!([]), 120);
    let straight = checkpoints(&rig);

    rig.ok(
        "replay.record",
        json!({"action": "start", "path": PATH, "checkpoint_every": 30}),
    );
    rig.ok("sim.step", json!({"ticks": 50}));
    let reload = rig.ok("script.reload", json!({}));
    assert_eq!(reload["modules"][0]["name"], "conformance", "{reload}");
    rig.ok("sim.step", json!({"ticks": 70}));
    rig.ok("replay.record", json!({"action": "stop"}));
    assert_eq!(checkpoints(&rig), straight, "the reload changed nothing");

    let run = rig.ok("replay.run", json!({"path": PATH}));
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["matched"], 5);
}

#[test]
fn recording_refuses_to_stop_when_a_module_changed_mid_recording() {
    let mut rig = scripted();
    rig.ok("replay.record", json!({"path": PATH}));
    rig.ok("sim.step", json!({"ticks": 10}));
    place(rig.dir.path(), "conformance_guest_v2");
    rig.ok("script.reload", json!({}));
    rig.ok("sim.step", json!({"ticks": 10}));
    let err = rig.err("replay.record", json!({"action": "stop"}));
    assert_eq!(err.kind, ApiErrorKind::ReplayMismatch, "{err:?}");
    assert_eq!(
        err.data.extra["mismatches"][0]["field"], "modules",
        "{err:?}"
    );
    assert!(err.message.contains("conformance"), "{err:?}");
    assert!(!rig.dir.path().join(PATH).exists(), "nothing written");
}
