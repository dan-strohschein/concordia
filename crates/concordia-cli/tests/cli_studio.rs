//! `concordia studio init|roster|status` and `concordia task ...` (M3 phase 3.3).

mod common;

use common::{run, stderr, stdout};
use serde_json::Value;

fn new_project(dir: &std::path::Path, extra: &[&str]) -> std::path::PathBuf {
    let mut args = vec!["new", "g"];
    args.extend_from_slice(extra);
    let output = run(&args, dir);
    assert!(output.status.success(), "{}", stderr(&output));
    dir.join("g")
}

fn json(output: &std::process::Output) -> Value {
    assert!(output.status.success(), "{}", stderr(output));
    serde_json::from_str(&stdout(output)).unwrap_or_else(|e| panic!("{e}: {}", stdout(output)))
}

#[test]
fn studio_roster_of_a_new_project_validates() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path(), &[]);
    let roster = json(&run(&["studio", "roster"], &root));
    assert_eq!(roster["director"], "director");
    let enabled: Vec<&str> = roster["roles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["role"].as_str().unwrap())
        .collect();
    for role in ["director", "level-designer", "qa-playtester"] {
        assert!(enabled.contains(&role), "{role} not enabled: {enabled:?}");
    }
    let disabled = roster["disabled"].as_array().unwrap();
    assert_eq!(enabled.len() + disabled.len(), 12);
    assert!(
        disabled
            .iter()
            .all(|d| !d["reason"].as_str().unwrap().is_empty()),
        "{disabled:?}"
    );
}

#[test]
fn task_cli_creates_lists_claims_and_defaults_to_the_local_human() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path(), &[]);
    let created = json(&run(
        &[
            "task",
            "create",
            "--title",
            "Build the arena",
            "--role",
            "level-designer",
        ],
        &root,
    ));
    assert_eq!(created["task"]["id"], "T-0001");
    let listed = json(&run(&["task", "list"], &root));
    assert_eq!(listed["tasks"][0]["title"], "Build the arena");
    let claimed = json(&run(
        &[
            "task",
            "claim",
            "T-0001",
            "--actor",
            "agent:level-designer#1",
        ],
        &root,
    ));
    assert_eq!(claimed["task"]["status"], "in_progress");
    let shown = json(&run(&["task", "show", "T-0001"], &root));
    assert_eq!(shown["history"][0]["actor"], "human:local");
}

#[test]
fn studio_init_adds_studio_to_an_existing_project_once() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path(), &["--no-studio"]);
    let output = run(&["studio", "init"], &root);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(root.join("studio/team.toml").is_file());
    let manifest = std::fs::read_to_string(root.join("concordia.toml")).unwrap();
    assert_eq!(manifest.matches("[studio]").count(), 1, "{manifest}");
    let again = run(&["studio", "init"], &root);
    assert_eq!(again.status.code(), Some(1));
    assert!(stderr(&again).contains("already"), "{}", stderr(&again));
}

#[test]
fn studio_commands_outside_a_studio_project_explain_how_to_start() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path(), &["--no-studio"]);
    let output = run(&["task", "list"], &root);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("concordia studio init"),
        "{}",
        stderr(&output)
    );
}
