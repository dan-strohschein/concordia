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
        text.replace("Give the game its words", "Write all text"),
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

/// Runs `concordia studio hook <event>` in `project` with `input` on stdin.
fn hook(project: &std::path::Path, event: &str, input: &serde_json::Value) -> std::process::Output {
    use std::io::Write;
    let mut child = common::concordia()
        .args(["studio", "hook", event])
        .current_dir(project)
        .env("CLAUDE_PROJECT_DIR", project)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

// Replaces 3.4's `studio_hook_passes_through_until_enforcement_lands` (deliberately: enforcement landed in 3.5).
#[test]
fn studio_hook_answers_claude_code_with_a_decision() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path(), &[]);
    let input = serde_json::json!({
        "session_id": "s", "hook_event_name": "PreToolUse", "agent_type": "writer", "agent_id": "a",
        "tool_name": "Edit", "tool_input": {"file_path": root.join("scenes/main.scene.ron")}, "tool_use_id": "t1"
    });
    let output = hook(&root, "pre-tool-use", &input);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let answer: Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(answer["hookSpecificOutput"]["permissionDecision"], "deny");
    let malformed = hook(&root, "pre-tool-use", &serde_json::json!("not an object"));
    assert_eq!(malformed.status.code(), Some(2), "fails closed");
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timing budget applies to release builds")]
fn hook_completes_under_50ms_p95() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path(), &[]);
    let input = serde_json::json!({
        "session_id": "s", "hook_event_name": "PreToolUse", "agent_type": "level-designer", "agent_id": "a",
        "tool_name": "mcp__concordia__entity_spawn", "tool_input": {"components": {}}, "tool_use_id": "t1"
    });
    let mut times: Vec<std::time::Duration> = (0..40)
        .map(|_| {
            let start = std::time::Instant::now();
            assert!(hook(&root, "pre-tool-use", &input).status.success());
            start.elapsed()
        })
        .collect();
    times.sort();
    let p95 = times[times.len() * 95 / 100];
    assert!(p95 < std::time::Duration::from_millis(50), "p95 {p95:?}");
}

#[test]
fn studio_questions_and_answer_cli() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path(), &[]);
    let journal = concordia_studio::StudioJournal::of_project(&root);
    journal
        .append(
            &"agent:level-designer#1".parse().unwrap(),
            concordia_studio::JournalEvent::QuestionAsked {
                question: "Q-0001".into(),
                to: "director".into(),
                text: "arena size?".into(),
                task: None,
                refs: Vec::new(),
            },
        )
        .unwrap();
    journal
        .append(
            &"agent:director#1".parse().unwrap(),
            concordia_studio::JournalEvent::QuestionEscalated {
                question: "Q-0001".into(),
                reason: "design call".into(),
            },
        )
        .unwrap();

    let escalated = json(&run(
        &["studio", "questions", "--status", "escalated"],
        &root,
    ));
    assert_eq!(escalated["questions"][0]["id"], "Q-0001");
    assert_eq!(escalated["questions"][0]["to"], "human");

    let answered = json(&run(&["studio", "answer", "Q-0001", "40 m square"], &root));
    assert_eq!(answered["question"]["status"], "answered");
    assert_eq!(answered["question"]["answer"]["by"], "human:local");
    assert_eq!(answered["question"]["answer"]["text"], "40 m square");
    let open = json(&run(&["studio", "questions", "--status", "open"], &root));
    assert_eq!(open["questions"], serde_json::json!([]));
}
