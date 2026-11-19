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

#[test]
fn new_project_is_synced_for_claude_code() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path(), &[]);
    assert!(root.join(".claude/agents/director.md").is_file());
    assert!(root.join(".claude/settings.json").is_file());
    let claude = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap();
    assert!(
        claude.contains("<!-- concordia-studio:begin -->"),
        "{claude}"
    );
    let check = run(&["studio", "sync", "--check"], &root);
    assert!(
        check.status.success(),
        "a fresh project is in sync: {}",
        stdout(&check)
    );
}

#[test]
fn concordia_studio_sync_dry_run_lists_changes_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path(), &["--no-studio"]);
    assert!(
        run(&["studio", "init", "--no-sync"], &root)
            .status
            .success()
    );
    let output = run(&["studio", "sync", "--dry-run"], &root);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains(".claude/agents/director.md"),
        "{}",
        stdout(&output)
    );
    assert!(!root.join(".claude").exists());
}

#[test]
fn studio_sync_check_exits_nonzero_and_prints_diff_when_stale() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path(), &[]);
    let role = root.join("studio/agents/writer.md");
    let text = std::fs::read_to_string(&role).unwrap();
    std::fs::write(
        &role,
        text.replace("Write the game's text", "Write all text"),
    )
    .unwrap();
    let output = run(&["studio", "sync", "--check"], &root);
    assert_eq!(output.status.code(), Some(1));
    let out = stdout(&output);
    assert!(
        out.contains("+Write all text") && out.contains("concordia studio sync"),
        "{out}"
    );
}

#[test]
fn studio_hook_passes_through_until_enforcement_lands() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let mut child = common::concordia()
        .args(["studio", "hook", "pre-tool-use"])
        .current_dir(dir.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"hook_event_name":"PreToolUse","tool_name":"Edit","tool_input":{}}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stdout.is_empty(),
        "no decision: Claude Code's own rules apply"
    );
}
