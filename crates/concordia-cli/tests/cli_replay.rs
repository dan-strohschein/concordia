//! `concordia replay record|run` (M2 phase 2.8) and `migrate` (M3 phase 3.1a).

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

/// `concordia replay migrate` rewrites a version 0 file as version 1 in place (the fields v0
/// never recorded stay out), and the migrated file still verifies; a v1 file is left as it is.
#[test]
fn replay_migrate_rewrites_version_0_as_version_1() {
    let dir = tempfile::tempdir().unwrap();
    let root = project(dir.path());
    let recorded = run(&["replay", "record", REPLAY, "--ticks", "60"], &root);
    assert!(recorded.status.success(), "{}", stderr(&recorded));
    let file = root.join(REPLAY);
    let v1 = std::fs::read_to_string(&file).unwrap();
    let v0 = concordia_tests::replays::as_version_0(&v1);
    std::fs::write(&file, &v0).unwrap();

    let migrated = run(&["replay", "migrate", REPLAY], &root);
    assert!(migrated.status.success(), "{}", stderr(&migrated));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("version: 1,") && !text.contains("hashes:"),
        "{text}"
    );
    assert!(
        stdout(&migrated).contains("rewritten as version 1"),
        "{}",
        stdout(&migrated)
    );
    let verified = run(&["replay", "run", REPLAY], &root);
    assert!(verified.status.success(), "{}", stderr(&verified));

    std::fs::write(&file, &v1).unwrap();
    let again = run(&["replay", "migrate", REPLAY], &root);
    assert!(again.status.success(), "{}", stderr(&again));
    assert!(
        stdout(&again).contains("already version 1"),
        "{}",
        stdout(&again)
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), v1);
}

/// `concordia replay diff a b` compares two replays offline (M3 3.1b): a recording against
/// itself is the same (exit 0); against one where Space is pressed on tick 5 instead of 3, the
/// first differing input tick, 3, is reported (exit 1). `run --mode resimulate --update` re-blesses a file,
/// and `run --mode ab --other <copy>` agrees with a copy of the project (3.1c).
#[test]
fn replay_diff_and_update_from_the_cli() {
    let dir = tempfile::tempdir().unwrap();
    let root = project(dir.path());
    let press = |name: &str, tick: u32| {
        std::fs::write(
            root.join(name),
            format!(
                r#"[{{"tick": {tick}, "event": {{"Key": {{"key": "Space", "pressed": true}}}}}}]"#
            ),
        )
        .unwrap();
    };
    press("a.json", 3);
    press("b.json", 5);
    for (file, inputs) in [
        ("tests/replays/a.replay.ron", "a.json"),
        ("tests/replays/b.replay.ron", "b.json"),
    ] {
        let out = run(
            &[
                "replay", "record", file, "--ticks", "60", "--inputs", inputs,
            ],
            &root,
        );
        assert!(out.status.success(), "{}", stderr(&out));
    }
    let same = run(
        &[
            "replay",
            "diff",
            "tests/replays/a.replay.ron",
            "tests/replays/a.replay.ron",
        ],
        &root,
    );
    assert!(same.status.success(), "{}", stderr(&same));
    assert_eq!(json(&same)["first_tick"], Value::Null);
    let differ = run(
        &[
            "replay",
            "diff",
            "tests/replays/a.replay.ron",
            "tests/replays/b.replay.ron",
        ],
        &root,
    );
    assert!(!differ.status.success());
    // Nothing in this project reacts to Space, so the hashes agree; the inputs don't, from 3.
    assert_eq!(json(&differ)["first_input_tick"], 3, "{}", stdout(&differ));

    let updated = run(
        &[
            "replay",
            "run",
            "tests/replays/a.replay.ron",
            "--mode",
            "resimulate",
            "--update",
        ],
        &root,
    );
    assert!(updated.status.success(), "{}", stderr(&updated));

    // ab against a copy of the project (3.1c): the same build agrees.
    let copy = dir.path().join("copy");
    concordia_tests::copy_project(&root, &copy).unwrap();
    let other = copy.to_str().unwrap();
    let args = [
        "replay",
        "run",
        "tests/replays/a.replay.ron",
        "--mode",
        "ab",
        "--other",
        other,
    ];
    let ab = run(&args, &root);
    assert!(ab.status.success(), "{}", stderr(&ab));
    assert_eq!(json(&ab)["ok"], true);
}
