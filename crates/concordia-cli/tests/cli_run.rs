//! `concordia run`: a windowed engine (M1 phase 1.1).

mod common;

use common::{run, stderr};

fn example(name: &str) -> String {
    format!("{}/../../examples/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn run_reports_an_invalid_manifest_before_opening_a_window() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), "[window]\nwidth = 0\n").unwrap();
    let output = run(
        &["run", "--project", dir.path().to_str().unwrap()],
        dir.path(),
    );
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("concordia.toml"),
        "{}",
        stderr(&output)
    );
}

/// Needs a desktop session (run by hand or on a GUI runner): `cargo nextest run --run-ignored only -E 'test(run_opens)'`.
#[test]
#[ignore = "opens a real window"]
fn run_opens_and_closes_window_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(
        &[
            "run",
            "--project",
            &example("m1-physics-stack"),
            "--exit-after-ms",
            "500",
        ],
        dir.path(),
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("window opened"),
        "{}",
        stderr(&output)
    );
}

/// Needs a desktop session with a GPU: `cargo nextest run --run-ignored only -E 'test(bench_latency)'`.
#[test]
#[ignore = "opens a real window"]
fn run_bench_latency_measures_the_engine_window_alone() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("latency.json");
    let output = run(
        &[
            "run",
            "--project",
            &example("m1-physics-stack"),
            "--bench-latency",
            out.to_str().unwrap(),
            "--bench-frames",
            "60",
        ],
        dir.path(),
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let presented = report["frames"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["presented_ms"].is_number() && f["input_time_ms"].is_number())
        .count();
    assert!(
        presented >= 30,
        "frames with input and display times: {presented}\n{report}"
    );
}

#[test]
fn run_plays_by_default_and_edit_keeps_the_editor() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(&["run", "--help"], dir.path());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(help.contains("--edit"), "{help}");
    assert!(help.contains("play its main scene"), "{help}");
}

/// Needs a desktop session: `cargo nextest run --run-ignored only -E 'test(in_its_window)'`.
#[test]
#[ignore = "opens a real window"]
fn run_plays_the_main_scene_in_its_window() {
    let dir = tempfile::tempdir().unwrap();
    let project = example("m1-physics-stack");
    let play = run(
        &["run", "--project", &project, "--exit-after-ms", "800"],
        dir.path(),
    );
    assert!(play.status.success(), "{}", stderr(&play));
    assert!(stderr(&play).contains("playing"), "{}", stderr(&play));
    let edit = run(
        &[
            "run",
            "--project",
            &project,
            "--edit",
            "--exit-after-ms",
            "500",
        ],
        dir.path(),
    );
    assert!(edit.status.success(), "{}", stderr(&edit));
    assert!(!stderr(&edit).contains("playing"), "{}", stderr(&edit));
}
