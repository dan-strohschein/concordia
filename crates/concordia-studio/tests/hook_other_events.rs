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

// --- Team questions (M3 phase 3.6b, ADR-0081): context injection, no polling.

fn record(root: &std::path::Path, actor: &str, event: concordia_studio::JournalEvent) {
    StudioJournal::of_project(root)
        .append(&actor.parse().unwrap(), event)
        .unwrap();
}

fn asked(id: &str, to: &str, text: &str) -> concordia_studio::JournalEvent {
    concordia_studio::JournalEvent::QuestionAsked {
        question: id.into(),
        to: to.into(),
        text: text.into(),
        task: None,
        refs: Vec::new(),
    }
}

fn answered(id: &str, text: &str) -> concordia_studio::JournalEvent {
    concordia_studio::JournalEvent::QuestionAnswered {
        question: id.into(),
        text: text.into(),
    }
}

fn context(outcome: &concordia_studio::HookOutcome, event: &str) -> String {
    assert_eq!(outcome.exit, 0, "{}", outcome.stderr);
    if outcome.stdout.is_empty() {
        return String::new();
    }
    let out: Value = serde_json::from_str(&outcome.stdout).unwrap();
    assert_eq!(out["hookSpecificOutput"]["hookEventName"], event, "{out}");
    out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn agent_returned() -> Value {
    let mut input = base("PostToolUse", None);
    input["tool_name"] = json!("Agent");
    input["tool_input"] = json!({"subagent_type": "level-designer", "prompt": "Work task T-0001"});
    input["tool_use_id"] = json!("toolu_9");
    input
}

#[test]
fn subagent_start_lists_open_questions_for_the_role_and_answers_to_mine() {
    let dir = project();
    let root = dir.path();
    record(
        root,
        "agent:level-designer#1",
        asked("Q-0001", "tech-artist", "metal or stone floor?"),
    );
    record(
        root,
        "agent:tech-artist#1",
        answered("Q-0001", "stone; T-0004 has the texture"),
    );
    record(
        root,
        "agent:qa-playtester#1",
        asked("Q-0002", "level-designer", "spawn height?"),
    );
    record(
        root,
        "agent:director#1",
        asked("Q-0003", "tech-artist", "not for you"),
    );

    let start = base("SubagentStart", Some("level-designer"));
    let text = context(
        &run(root, HookEvent::SubagentStart, &start),
        "SubagentStart",
    );
    assert!(
        text.contains("Q-0002") && text.contains("spawn height?"),
        "{text}"
    );
    assert!(text.contains("from agent:qa-playtester#1"), "{text}");
    assert!(
        text.contains("Q-0001") && text.contains("stone; T-0004 has the texture"),
        "answers to my role's questions: {text}"
    );
    assert!(!text.contains("Q-0003"), "{text}");
    assert!(text.contains("studio_answer"), "says how to answer: {text}");

    // Answers are delivered once per role; open questions stay listed until answered.
    let again = context(
        &run(root, HookEvent::SubagentStart, &start),
        "SubagentStart",
    );
    assert!(!again.contains("stone; T-0004"), "{again}");
    assert!(again.contains("Q-0002"), "{again}");
}

#[test]
fn post_tool_use_on_agent_returns_new_questions_to_the_director() {
    let dir = project();
    let root = dir.path();
    record(
        root,
        "agent:level-designer#1",
        asked("Q-0001", "tech-artist", "metal or stone floor?"),
    );
    let text = context(
        &run(root, HookEvent::PostToolUse, &agent_returned()),
        "PostToolUse",
    );
    assert!(
        text.contains("Q-0001") && text.contains("metal or stone floor?"),
        "{text}"
    );
    assert!(
        text.contains("agent:level-designer#1") && text.contains("tech-artist"),
        "{text}"
    );
    assert!(
        text.contains("studio_escalate"),
        "says how to route: {text}"
    );

    // Next turn: only what changed since.
    record(root, "agent:tech-artist#1", answered("Q-0001", "stone"));
    record(
        root,
        "agent:qa-playtester#1",
        asked("Q-0002", "director", "retest T-0001?"),
    );
    let next = context(
        &run(root, HookEvent::PostToolUse, &agent_returned()),
        "PostToolUse",
    );
    assert!(
        next.contains("Q-0002") && next.contains("Q-0001") && next.contains("stone"),
        "{next}"
    );
    assert!(
        !next.contains("metal or stone floor?"),
        "already seen: {next}"
    );
    let quiet = run(root, HookEvent::PostToolUse, &agent_returned());
    assert!(
        quiet.stdout.is_empty(),
        "nothing new, no context: {}",
        quiet.stdout
    );
}

#[test]
fn post_tool_use_on_agent_says_nothing_to_a_subagent() {
    let dir = project();
    let root = dir.path();
    record(
        root,
        "agent:level-designer#1",
        asked("Q-0001", "director", "go?"),
    );
    let mut input = agent_returned();
    input["agent_type"] = json!("level-designer");
    input["agent_id"] = json!("a-1");
    assert!(run(root, HookEvent::PostToolUse, &input).stdout.is_empty());
}

#[test]
fn user_prompt_submit_injects_new_human_answers() {
    let dir = project();
    let root = dir.path();
    record(
        root,
        "agent:level-designer#1",
        asked("Q-0001", "director", "arena size?"),
    );
    record(
        root,
        "agent:director#1",
        concordia_studio::JournalEvent::QuestionEscalated {
            question: "Q-0001".into(),
            reason: "design call".into(),
        },
    );
    let mut prompt = base("UserPromptSubmit", None);
    prompt["prompt"] = json!("carry on");
    let first = run(root, HookEvent::UserPromptSubmit, &prompt);
    assert!(first.stdout.is_empty() || !context(&first, "UserPromptSubmit").contains("40 m"));

    record(root, "human:local", answered("Q-0001", "40 m square"));
    let text = context(
        &run(root, HookEvent::UserPromptSubmit, &prompt),
        "UserPromptSubmit",
    );
    assert!(
        text.contains("Q-0001") && text.contains("40 m square"),
        "{text}"
    );
    assert!(text.contains("human:local"), "{text}");
    let again = run(root, HookEvent::UserPromptSubmit, &prompt);
    assert!(again.stdout.is_empty(), "delivered once: {}", again.stdout);
    assert_eq!(
        HookEvent::from_cli("user-prompt-submit"),
        Some(HookEvent::UserPromptSubmit)
    );
}
