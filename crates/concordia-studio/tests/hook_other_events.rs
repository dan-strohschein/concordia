//! `concordia studio hook` for PostToolUse, SubagentStart and SubagentStop (M3 phase 3.5).

mod common;

use common::hook::{base, pre, project, run};
use concordia_studio::{HookEvent, Slots, StudioJournal};
use serde_json::{Value, json};

#[test]
fn post_tool_use_edit_records_a_file_edit_with_actor_and_hashes() {
    let dir = project();
    let path = dir.path().join("content/intro.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "old").unwrap();
    let input = pre(Some("writer"), "Edit", json!({"file_path": path}));
    run(dir.path(), HookEvent::PreToolUse, &input);
    std::fs::write(&path, "new").unwrap();

    let mut post = base("PostToolUse", Some("writer"));
    post["tool_name"] = json!("Edit");
    post["tool_input"] = json!({"file_path": path});
    post["tool_use_id"] = json!("toolu_1");
    post["tool_response"] = json!({"success": true});
    let outcome = run(dir.path(), HookEvent::PostToolUse, &post);
    assert_eq!(outcome.exit, 0, "{}", outcome.stderr);

    let entries = StudioJournal::of_project(dir.path()).read(None).unwrap();
    let edit = entries
        .iter()
        .find(|e| e.event.kind() == "file_edit")
        .unwrap()
        .to_json();
    assert_eq!(edit["actor"], "agent:writer");
    assert_eq!(edit["path"], "content/intro.txt");
    assert_eq!(edit["tool"], "Edit");
    assert!(
        edit["before"].is_string() && edit["after"].is_string() && edit["before"] != edit["after"],
        "{edit}"
    );
}

#[test]
fn post_tool_use_names_the_instance_once_the_engine_assigned_one() {
    let dir = project();
    Slots::of_project(dir.path())
        .claim("writer", "s-1/a-writer", 2, std::process::id())
        .unwrap();
    let path = dir.path().join("content/a.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "x").unwrap();
    let mut post = base("PostToolUse", Some("writer"));
    post["tool_name"] = json!("Write");
    post["tool_input"] = json!({"file_path": path});
    post["tool_use_id"] = json!("toolu_9");
    run(dir.path(), HookEvent::PostToolUse, &post);
    let entries = StudioJournal::of_project(dir.path()).read(None).unwrap();
    assert_eq!(entries.last().unwrap().actor.to_string(), "agent:writer#1");
}

#[test]
fn subagent_start_injects_the_actor_and_its_tasks() {
    let dir = project();
    let tasks = dir.path().join("studio/tasks");
    std::fs::write(
        tasks.join("T-0001-build-the-arena.task.md"),
        "---\nid: T-0001\nuid: u\ntitle: Build the arena\nstatus: todo\nrole: level-designer\npriority: normal\ncreated_by: agent:director\n---\nWalls.\n\n## Notes\n\n## Review\n",
    )
    .unwrap();
    let outcome = run(
        dir.path(),
        HookEvent::SubagentStart,
        &base("SubagentStart", Some("level-designer")),
    );
    assert_eq!(outcome.exit, 0, "{}", outcome.stderr);
    let out: Value = serde_json::from_str(&outcome.stdout).unwrap();
    assert_eq!(out["hookSpecificOutput"]["hookEventName"], "SubagentStart");
    let context = out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(
        context.contains("agent:level-designer") && context.contains("T-0001 Build the arena"),
        "{context}"
    );
}

#[test]
fn subagent_stop_releases_the_instance_slot() {
    let dir = project();
    let slots = Slots::of_project(dir.path());
    slots
        .claim("writer", "s-1/a-writer", 1, std::process::id())
        .unwrap();
    assert!(
        slots
            .claim("writer", "s-1/other", 1, std::process::id())
            .is_err(),
        "full"
    );
    let outcome = run(
        dir.path(),
        HookEvent::SubagentStop,
        &base("SubagentStop", Some("writer")),
    );
    assert_eq!(outcome.exit, 0, "{}", outcome.stderr);
    assert_eq!(
        slots
            .claim("writer", "s-1/other", 1, std::process::id())
            .unwrap()
            .get(),
        1
    );
}

#[test]
fn slots_held_by_a_dead_server_are_reclaimed() {
    let dir = project();
    let slots = Slots::of_project(dir.path());
    let dead = 4_000_000; // above macOS and Linux pid limits
    slots.claim("writer", "old/agent", 1, dead).unwrap();
    assert_eq!(
        slots
            .claim("writer", "new/agent", 1, std::process::id())
            .unwrap()
            .get(),
        1
    );
}

#[test]
fn session_end_releases_every_slot_the_session_held() {
    let dir = project();
    let slots = Slots::of_project(dir.path());
    slots
        .claim("director", "s-1/main", 1, std::process::id())
        .unwrap();
    slots
        .claim("writer", "s-1/a-writer", 1, std::process::id())
        .unwrap();
    slots
        .claim("writer", "s-2/a-writer", 2, std::process::id())
        .unwrap();
    let mut input = base("SessionEnd", None);
    input["reason"] = json!("other");
    let outcome = run(dir.path(), HookEvent::SessionEnd, &input);
    assert_eq!(outcome.exit, 0, "{}", outcome.stderr);
    assert_eq!(slots.lookup("s-1/main").unwrap(), None);
    assert_eq!(slots.lookup("s-1/a-writer").unwrap(), None);
    assert!(
        slots.lookup("s-2/a-writer").unwrap().is_some(),
        "other sessions keep theirs"
    );
}
