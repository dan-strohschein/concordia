//! Fixtures for `concordia studio hook`: a synced project and hook inputs shaped like Claude Code's.
#![allow(dead_code)]

use std::path::Path;

use concordia_studio::{HookEvent, HookOutcome, SyncOptions, ToolCatalog, hook, sync};
use serde_json::{Value, json};

/// The catalog with the read-only tools marked (what the engine reports).
pub fn catalog() -> ToolCatalog {
    let read_only = super::PROJECT_TOOLS.iter().copied().filter(|t| {
        [
            "_get",
            "_list",
            "_info",
            "_history",
            "_tail",
            "_run",
            "_stats",
            "_templates",
            "_roster",
            "_status",
            "_activity",
            "_files",
            "_raycast",
        ]
        .iter()
        .any(|s| t.ends_with(s))
            || *t == "capture_screenshot"
    });
    super::catalog().with_read_only(read_only)
}

/// A synced project with the default team, plus a bash-scoped gameplay programmer enabled.
pub fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    concordia_studio::init(dir.path(), Path::new("studio")).unwrap();
    let team = dir.path().join("studio/team.toml");
    let text = std::fs::read_to_string(&team).unwrap().replace(
        "[roles.gameplay-programmer]\nenabled = false",
        "[roles.gameplay-programmer]\nenabled = true",
    );
    std::fs::write(&team, text).unwrap();
    resync(dir.path());
    dir
}

/// Regenerates the Claude Code files (and the policy) after editing `studio/`.
pub fn resync(root: &Path) {
    let roster = concordia_studio::Roster::load(&root.join("studio"), &catalog()).unwrap();
    sync(
        root,
        Path::new("studio"),
        &roster,
        &catalog(),
        SyncOptions::default(),
    )
    .unwrap();
}

/// Common hook input fields; `agent_type` none means the main session.
pub fn base(event: &str, agent_type: Option<&str>) -> Value {
    let mut input = json!({
        "session_id": "s-1",
        "transcript_path": "/tmp/t.jsonl",
        "cwd": "/somewhere",
        "hook_event_name": event,
    });
    if let Some(agent) = agent_type {
        input["agent_type"] = json!(agent);
        input["agent_id"] = json!(format!("a-{agent}"));
    }
    input
}

/// A PreToolUse input for `tool` with `tool_input`.
pub fn pre(agent_type: Option<&str>, tool: &str, tool_input: Value) -> Value {
    let mut input = base("PreToolUse", agent_type);
    input["tool_name"] = json!(tool);
    input["tool_input"] = tool_input;
    input["tool_use_id"] = json!("toolu_1");
    input
}

/// Runs the hook with `input` for the project at `root`.
pub fn run(root: &Path, event: HookEvent, input: &Value) -> HookOutcome {
    hook::run(event, &input.to_string(), root)
}

/// The PreToolUse decision: ("allow" | "deny" | "", reason, updatedInput).
pub fn decision(outcome: &HookOutcome) -> (String, String, Value) {
    assert_eq!(outcome.exit, 0, "stderr: {}", outcome.stderr);
    if outcome.stdout.is_empty() {
        return (String::new(), String::new(), Value::Null);
    }
    let out: Value = serde_json::from_str(&outcome.stdout).unwrap();
    let specific = &out["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], "PreToolUse");
    (
        specific["permissionDecision"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        specific["permissionDecisionReason"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        specific["updatedInput"].clone(),
    )
}
