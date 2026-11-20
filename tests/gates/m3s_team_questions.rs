//! Studio checkpoint gate (M3 phase 3.6b, ADR-0081): a scripted twin of the live team-questions loop.
//!
//! Hook inputs shaped like Claude Code's go through the real `concordia studio hook` binary, and
//! the hook's `updatedInput` goes to a real `concordia mcp`. A level designer asks the tech artist;
//! the director hears of it and the tech artist answers; another question is escalated, a human
//! answers over the Engine API, and the task unblocks. No live agent, no API key.

use std::path::Path;

use concordia_api::EngineLink;
use concordia_app::SharedEngine;
use concordia_studio::{JournalEntry, StudioJournal};
use concordia_tests::studio_gate::{agent_call, context, hook, input, mcp, new_project};
use serde_json::json;

fn journal(root: &Path) -> Vec<JournalEntry> {
    StudioJournal::of_project(root).read(None).unwrap()
}

fn find<'a>(entries: &'a [JournalEntry], kind: &str, question: &str) -> &'a JournalEntry {
    entries
        .iter()
        .find(|e| e.event.kind() == kind && e.to_json()["question"] == question)
        .unwrap_or_else(|| panic!("no {kind} for {question}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn m3s_team_questions_scripted_loop() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    let mcp = mcp(&root).await;

    // The director plans a task; the level designer claims it and asks the tech artist.
    let task = agent_call(
        &root,
        &mcp,
        None,
        "task_create",
        json!({"title": "Arena floor", "role": "level-designer"}),
    )
    .await["task"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    agent_call(
        &root,
        &mcp,
        Some("level-designer"),
        "task_claim",
        json!({"id": task}),
    )
    .await;
    let q1 = agent_call(
        &root,
        &mcp,
        Some("level-designer"),
        "studio_ask",
        json!({"to": "tech-artist", "text": "floor material? stone or metal plate?", "task": task}),
    )
    .await["question"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // The level designer returns; the director's PostToolUse on Agent lists the new question.
    let mut returned = input("PostToolUse", None);
    returned["tool_name"] = json!("Agent");
    returned["tool_input"] = json!({"subagent_type": "level-designer"});
    let director_ctx = context(&hook(&root, "post-tool-use", &returned));
    assert!(
        director_ctx.contains(&q1) && director_ctx.contains("tech-artist"),
        "{director_ctx}"
    );

    // The director routes it: the tech artist starts, sees the question and answers it.
    let start = context(&hook(
        &root,
        "subagent-start",
        &input("SubagentStart", Some("tech-artist")),
    ));
    assert!(
        start.contains(&q1) && start.contains("floor material?"),
        "{start}"
    );
    agent_call(
        &root,
        &mcp,
        Some("tech-artist"),
        "studio_answer",
        json!({"id": q1, "text": "stone; assets/materials/stone_floor.material.ron"}),
    )
    .await;

    // The level designer, re-run, gets the answer at start.
    let rerun = context(&hook(
        &root,
        "subagent-start",
        &input("SubagentStart", Some("level-designer")),
    ));
    assert!(rerun.contains("stone_floor"), "{rerun}");

    // A design question nobody on the team can answer: the director escalates it.
    let q2 = agent_call(
        &root,
        &mcp,
        Some("level-designer"),
        "studio_ask",
        json!({"to": "director", "text": "arena size? 30 or 40 m?", "task": task}),
    )
    .await["question"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut prompt = input("UserPromptSubmit", None);
    prompt["prompt"] = json!("status?");
    hook(&root, "user-prompt-submit", &prompt);
    agent_call(
        &root,
        &mcp,
        None,
        "studio_escalate",
        json!({"id": q2, "reason": "arena scale is a design call"}),
    )
    .await;
    let blocked = agent_call(&root, &mcp, None, "task_get", json!({"id": task})).await;
    assert_eq!(blocked["task"]["status"], "blocked");

    // The human answers over the Engine API (as the editor's panel does), on the shared engine.
    let modules = concordia_app::Project::load(&root)
        .unwrap()
        .modules()
        .unwrap();
    let engine = SharedEngine::attach_live(&root, &modules)
        .await
        .unwrap()
        .expect("concordia mcp published its engine");
    let human = engine.open_session().await.unwrap();
    engine
        .call(
            human,
            "session.open",
            json!({"client": "gate", "actor": "human:local"}),
        )
        .await
        .unwrap();
    engine
        .call(
            human,
            "studio.answer",
            json!({"id": q2, "text": "40 m square"}),
        )
        .await
        .unwrap();

    let unblocked = agent_call(&root, &mcp, None, "task_get", json!({"id": task})).await;
    assert_eq!(unblocked["task"]["status"], "todo");
    let answers = context(&hook(&root, "user-prompt-submit", &prompt));
    assert!(
        answers.contains(&q2) && answers.contains("40 m square"),
        "{answers}"
    );

    // The journal holds who asked, answered and escalated what, and when.
    let entries = journal(&root);
    assert_eq!(
        find(&entries, "question_asked", &q1).actor.to_string(),
        "agent:level-designer#1"
    );
    assert_eq!(
        find(&entries, "question_answered", &q1).actor.to_string(),
        "agent:tech-artist#1"
    );
    assert_eq!(
        find(&entries, "question_escalated", &q2).actor.to_string(),
        "agent:director#1"
    );
    assert_eq!(
        find(&entries, "question_answered", &q2).actor.to_string(),
        "human:local"
    );
    let block = entries
        .iter()
        .find(|e| e.to_json()["changes"]["status"] == json!(["in_progress", "blocked"]))
        .expect("the escalation blocked the task");
    assert_eq!(block.actor.to_string(), "system:studio");
    assert!(
        entries.windows(2).all(|w| w[0].ts_ms <= w[1].ts_ms),
        "times in order"
    );
    mcp.shutdown().await;
}
