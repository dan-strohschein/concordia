//! `concordia assert eval` and `concordia test playtests` (M3 phase 3.2b). A new project's main
//! scene is empty, so `count(with(Name))` is 0 on every tick of a 30-tick recording.

mod common;

use common::{run, stderr, stdout};
use serde_json::Value;

const REPLAY: &str = "tests/replays/idle.replay.ron";

fn project(dir: &std::path::Path) -> std::path::PathBuf {
    let output = run(&["new", "g", "--no-studio"], dir);
    assert!(output.status.success(), "{}", stderr(&output));
    let root = dir.join("g");
    let rec = run(&["replay", "record", REPLAY, "--ticks", "30"], &root);
    assert!(rec.status.success(), "{}", stderr(&rec));
    root
}

/// Exit 0 when every assertion passes, 1 when one fails, 2 when one doesn't parse or check;
/// `--json` prints the result object.
#[test]
fn concordia_assert_eval_exit_code_reflects_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let root = project(dir.path());
    let pass = run(
        &[
            "assert",
            "eval",
            "always: count(with(Name)) == 0",
            "--replay",
            REPLAY,
            "--json",
        ],
        &root,
    );
    assert_eq!(pass.status.code(), Some(0), "{}", stderr(&pass));
    let json: Value = serde_json::from_str(&stdout(&pass)).unwrap();
    assert_eq!(
        json["results"][0]["decided_at"], 30,
        "always passes at the end"
    );
    let fail = run(
        &[
            "assert",
            "eval",
            "always: count(with(Name)) > 0",
            "--replay",
            REPLAY,
        ],
        &root,
    );
    assert_eq!(fail.status.code(), Some(1), "{}", stderr(&fail));
    assert!(
        stdout(&fail).contains("failed at tick 0"),
        "{}",
        stdout(&fail)
    );
    let bad = run(
        &[
            "assert",
            "eval",
            "within 5 tickz: event(\"A\")",
            "--replay",
            REPLAY,
        ],
        &root,
    );
    assert_eq!(bad.status.code(), Some(2), "{}", stderr(&bad));
    assert!(stderr(&bad).contains("tickz"), "{}", stderr(&bad));
}

/// `concordia test playtests` runs every `tests/playtests/**/*.playtest.ron`: all passing is
/// exit 0; one failing is exit 1, naming it.
#[test]
fn concordia_test_playtests_runs_every_suite() {
    let dir = tempfile::tempdir().unwrap();
    let root = project(dir.path());
    let suite = |name: &str, assertion: &str| {
        let file = root.join(format!("tests/playtests/{name}.playtest.ron"));
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            file,
            format!("Playtest(\n    scene: \"scenes/main.scene.ron\",\n    max_ticks: 10,\n    assertions: [{assertion:?}],\n)\n"),
        )
        .unwrap();
    };
    suite("empty", "always: count(with(Name)) == 0");
    let ok = run(&["test", "playtests"], &root);
    assert_eq!(ok.status.code(), Some(0), "{}{}", stdout(&ok), stderr(&ok));
    assert!(stdout(&ok).contains("empty"), "{}", stdout(&ok));
    suite("crowded", "within 10 ticks: count(with(Name)) > 3");
    let failed = run(&["test", "playtests"], &root);
    assert_eq!(failed.status.code(), Some(1), "{}", stdout(&failed));
    assert!(stderr(&failed).contains("crowded"), "{}", stderr(&failed));
}
