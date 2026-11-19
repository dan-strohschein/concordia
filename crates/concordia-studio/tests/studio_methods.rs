//! `studio.roster`, `studio.status`, `studio.activity`, events and method schemas (M3 phase 3.3).

mod common;

use std::sync::{Arc, Mutex};

use common::Fixture;
use concordia_api::{Topic, params_schema};
use concordia_studio::{JournalEvent, StudioJournal, StudioModule};
use serde_json::{Value, json};

#[test]
fn roster_expands_tool_globs_against_the_engine_methods() {
    let mut fx = Fixture::new();
    let roster = fx.ok("studio.roster", json!({}));
    assert_eq!(roster["director"], "director");
    let designer = roster["roles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["role"] == "level-designer")
        .unwrap()
        .clone();
    let tools: Vec<&str> = designer["mcp_tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    // Only the Studio module is loaded here, so `scene_*` and friends match the core tools.
    assert!(
        tools.contains(&"scene_open") && tools.contains(&"task_claim"),
        "{tools:?}"
    );
    assert!(tools.windows(2).all(|w| w[0] < w[1]), "sorted: {tools:?}");
    assert!(
        !tools
            .iter()
            .any(|t| t.starts_with("events_") || *t == "session_open"),
        "{tools:?}"
    );
    assert_eq!(designer["count"], 3);
    assert_eq!(
        roster["disabled"],
        json!([{"role": "writer", "reason": "no text assets yet"}])
    );
}

#[test]
fn status_counts_tasks_and_lists_pending_approvals() {
    let mut fx = Fixture::new();
    let a = fx.create("one");
    fx.create("two");
    fx.submit(&a, "agent:level-designer#1");
    let status = fx.ok("studio.status", json!({}));
    assert_eq!(status["tasks"]["todo"], 1);
    assert_eq!(status["tasks"]["in_review"], 1);
    assert_eq!(status["pending_approvals"], json!([a]));
}

#[test]
fn file_edit_entries_appended_out_of_process_appear_in_activity() {
    let mut fx = Fixture::new();
    // What a PostToolUse hook does (ADR-0080): append straight to the Studio journal.
    let journal = StudioJournal::new(fx.path(".concordia/studio/journal.jsonl"));
    journal
        .append(
            &"agent:level-designer#1".parse().unwrap(),
            JournalEvent::FileEdit {
                path: "scenes/arena.scene.ron".into(),
                tool: "Write".into(),
                before: None,
                after: Some("ab12".into()),
            },
        )
        .unwrap();
    let entries = fx.ok("studio.activity", json!({"kind": "file_edit"}))["entries"].clone();
    assert_eq!(entries.as_array().unwrap().len(), 1);
    assert_eq!(entries[0]["actor"], "agent:level-designer#1");
    assert_eq!(entries[0]["path"], "scenes/arena.scene.ron");
}

#[test]
fn activity_filters_by_actor_role_task_and_kind_and_pages_with_since() {
    let mut fx = Fixture::new();
    let a = fx.create("one");
    let b = fx.create("two");
    fx.ok(
        "task.claim",
        json!({"id": a, "actor": "agent:level-designer#1"}),
    );
    fx.ok(
        "task.comment",
        json!({"id": b, "text": "hi", "actor": "agent:qa-playtester#1"}),
    );
    let count = |fx: &mut Fixture, filter: Value| {
        fx.ok("studio.activity", filter)["entries"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(count(&mut fx, json!({})), 4);
    assert_eq!(count(&mut fx, json!({"actor": "agent:director"})), 2);
    assert_eq!(count(&mut fx, json!({"role": "level-designer"})), 1);
    assert_eq!(count(&mut fx, json!({"task": b})), 2);
    assert_eq!(count(&mut fx, json!({"kind": "task_comment"})), 1);
    let first = fx.ok("studio.activity", json!({"limit": 2}));
    assert_eq!(first["entries"].as_array().unwrap().len(), 2);
    assert_eq!(count(&mut fx, json!({"since": first["next"]})), 2);
}

#[test]
fn api_task_changes_publish_studio_events() {
    let mut fx = Fixture::new();
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let sink = Arc::clone(&seen);
    fx.engine.set_event_sink(
        fx.session,
        Box::new(move |event| sink.lock().unwrap().push(event)),
    );
    fx.ok("events.subscribe", json!({"topics": ["studio"]}));
    fx.create("one");
    let events = seen.lock().unwrap().clone();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0]["params"]["topic"], json!(Topic::studio));
    assert_eq!(events[0]["params"]["kind"], "task_created");
}

#[test]
fn studio_method_schemas_snapshot() {
    let module = StudioModule::new(std::path::Path::new("."), "studio");
    let schemas: Vec<Value> = concordia_api::EngineModule::methods(&module)
        .iter()
        .map(|m| json!({"name": m.name, "mutates": m.mutates, "docs": m.docs, "params": params_schema(m)}))
        .collect();
    insta::assert_json_snapshot!(schemas);
}

#[test]
fn a_project_without_studio_files_reports_how_to_add_them() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = common::engine(dir.path());
    let s = engine.open_session();
    let err = engine.call(s, "studio.roster", json!({})).unwrap_err();
    assert!(err.message.contains("studio/team.toml"), "{}", err.message);
    assert!(
        err.data
            .suggestions
            .iter()
            .any(|s| s.contains("concordia studio init")),
        "{:?}",
        err.data.suggestions
    );
}

#[test]
fn studio_sync_writes_claude_code_files_and_journals_a_sync_entry() {
    let mut fx = Fixture::new();
    let report = fx.ok("studio.sync", json!({"actor": "human:local"}));
    assert_eq!(report["stale"], false);
    assert!(fx.path(".claude/agents/director.md").is_file());
    assert!(fx.path(".claude/agents/level-designer.md").is_file());
    assert!(
        !fx.path(".claude/agents/writer.md").exists(),
        "writer is disabled in the fixture team"
    );
    let entries = fx.ok("studio.activity", json!({"kind": "sync"}))["entries"].clone();
    assert_eq!(entries.as_array().unwrap().len(), 1);
    assert_eq!(entries[0]["actor"], "human:local");
    assert!(
        entries[0]["changed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == ".mcp.json")
    );

    let check = fx.ok(
        "studio.sync",
        json!({"check": true, "actor": "human:local"}),
    );
    assert_eq!(check["stale"], false);
    assert_eq!(
        fx.ok("studio.activity", json!({"kind": "sync"}))["entries"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "an up-to-date sync journals nothing"
    );
}
