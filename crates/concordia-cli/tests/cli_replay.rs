//! `concordia replay record|run` (M2 phase 2.8).

mod common;

use common::{run, stderr, stdout};
use serde_json::Value;

const MAP: &str = include_str!("../../../examples/m1-physics-stack/input/default.input.ron");
const REPLAY: &str = "tests/replays/cli.replay.ron";

/// A new project (no Studio) with the example's input map.
fn project(dir: &std::path::Path) -> std::path::PathBuf {
    let output = run(&["new", "g", "--no-studio"], dir);
    assert!(output.status.success(), "{}", stderr(&output));
    let root = dir.join("g");
    std::fs::write(root.join("default.input.ron"), MAP).unwrap();
    let manifest = root.join("concordia.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    // New projects point [input] at the starter map; this test's map replaces it.
    std::fs::write(
        &manifest,
        text.replace("input/default.input.ron", "default.input.ron"),
    )
    .unwrap();
    root
}

fn json(output: &std::process::Output) -> Value {
    serde_json::from_str(&stdout(output)).unwrap_or_else(|e| panic!("{e}: {}", stdout(output)))
}

#[test]
fn replay_record_and_run_a_scripted_scenario_from_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    let root = project(dir.path());
    std::fs::write(
        root.join("inputs.json"),
        r#"[{"tick": 3, "event": {"Key": {"key": "Space", "pressed": true}}},
            {"tick": 9, "event": {"Key": {"key": "Space", "pressed": false}}}]"#,
    )
    .unwrap();
    let recorded = run(
        &[
            "replay",
            "record",
            REPLAY,
            "--ticks",
            "120",
            "--inputs",
            "inputs.json",
            "--checkpoint-every",
            "30",
        ],
        &root,
    );
    assert!(recorded.status.success(), "{}", stderr(&recorded));
    let recorded = json(&recorded);
    assert_eq!(
        (recorded["tick"].clone(), recorded["inputs"].clone()),
        (120.into(), 4.into())
    );
    assert!(root.join(REPLAY).is_file());

    let verified = run(&["replay", "run", REPLAY, "--threads", "4"], &root);
    assert!(verified.status.success(), "{}", stderr(&verified));
    let verified = json(&verified);
    assert_eq!(
        (verified["ok"].clone(), verified["matched"].clone()),
        (true.into(), 5.into())
    );

    // Another seed: refused, naming the field and --force; forced, it diverges at tick 0.
    let manifest = root.join("concordia.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, text.replace("seed = 0", "seed = 1")).unwrap();
    let refused = run(&["replay", "run", REPLAY], &root);
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("seed") && stderr(&refused).contains("--force"),
        "{}",
        stderr(&refused)
    );
    let forced = run(&["replay", "run", REPLAY, "--force"], &root);
    assert!(!forced.status.success(), "a divergence fails the command");
    let forced = json(&forced);
    assert_eq!(forced["divergence"]["tick"], 0, "{forced}");
    assert_eq!(forced["divergence"]["parts"], serde_json::json!(["@rng"]));
}
