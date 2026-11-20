//! Studio checkpoint gate (ADR-0049, ADR-0080): the task board's state machine and approval gates
//! hold end to end. Agents act through the real hook binary and `concordia mcp`; the human acts over
//! the Engine API on the same shared engine, as the editor does. History is the Studio journal.

use std::path::Path;

use concordia_api::EngineLink;
use concordia_api::transport::remote::RemoteEngine;
use concordia_app::SharedEngine;
use concordia_core::id::SessionId;
use concordia_studio::StudioJournal;
use concordia_tests::mcp_client::McpClient;
use concordia_tests::studio_gate::{agent_call, agent_try, mcp, new_project};
use serde_json::{Value, json};

const LD: Option<&str> = Some("level-designer");
const QA: Option<&str> = Some("qa-playtester");

async fn create(root: &Path, mcp: &McpClient, title: &str) -> String {
    let created = agent_call(
        root,
        mcp,
        None,
        "task_create",
        json!({"title": title, "role": "level-designer"}),
    )
    .await;
    created["task"]["id"].as_str().unwrap().to_owned()
}

async fn submit(root: &Path, mcp: &McpClient, id: &str) {
    agent_call(root, mcp, LD, "task_claim", json!({"id": id})).await;
    agent_call(
        root,
        mcp,
        LD,
        "task_comment",
        json!({"id": id, "text": "built it; screenshot attached"}),
    )
    .await;
    agent_call(
        root,
        mcp,
        LD,
        "task_update",
        json!({"id": id, "status": "in_review"}),
    )
    .await;
}

async fn status(root: &Path, mcp: &McpClient, id: &str) -> Value {
    agent_call(root, mcp, None, "task_get", json!({"id": id})).await["task"].clone()
}

/// A `human:local` session on the engine `concordia mcp` hosts (the editor's view).
async fn human(root: &Path) -> (RemoteEngine, SessionId) {
    let modules = concordia_app::Project::load(root)
        .unwrap()
        .modules()
        .unwrap();
    let engine = SharedEngine::attach_live(root, &modules)
        .await
        .unwrap()
        .expect("concordia mcp published its engine");
    let session = engine.open_session().await.unwrap();
    engine
        .call(
            session,
            "session.open",
            json!({"client": "gate", "actor": "human:local"}),
        )
        .await
        .unwrap();
    (engine, session)
}

fn actors(root: &Path, task: &str) -> Vec<(String, String)> {
    StudioJournal::of_project(root)
        .read(None)
        .unwrap()
        .iter()
        .map(|e| e.to_json())
        .filter(|e| e["task"] == task)
        .map(|e| {
            (
                e["kind"].as_str().unwrap().to_owned(),
                e["actor"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn m3s_task_flows_through_the_gates_with_attribution() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    let mcp = mcp(&root).await;
    let id = create(&root, &mcp, "Arena floor").await;

    let early = agent_try(
        &root,
        &mcp,
        LD,
        "task_update",
        json!({"id": id, "status": "in_review"}),
    )
    .await;
    assert!(early.is_err(), "todo → in_review skips the claim");
    let done = agent_try(
        &root,
        &mcp,
        LD,
        "task_update",
        json!({"id": id, "status": "done"}),
    )
    .await;
    assert!(done.is_err(), "done goes through task_approve");

    submit(&root, &mcp, &id).await;
    assert_eq!(status(&root, &mcp, &id).await["status"], "in_review");
    let own = agent_try(&root, &mcp, None, "task_approve", json!({"id": id}))
        .await
        .unwrap_err();
    assert!(
        own.text.contains("qa-playtester"),
        "the director is not a gate party: {}",
        own.text
    );

    agent_call(
        &root,
        &mcp,
        QA,
        "task_approve",
        json!({"id": id, "comment": "floor present, lit"}),
    )
    .await;
    let task = status(&root, &mcp, &id).await;
    assert_eq!(task["status"], "done");

    let history = actors(&root, &id);
    let who = |kind: &str| {
        history
            .iter()
            .filter(|(k, _)| k == kind)
            .map(|(_, a)| a.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(who("task_created"), ["agent:director#1"]);
    assert_eq!(who("task_comment"), ["agent:level-designer#1"]);
    assert!(
        who("task_changed")
            .iter()
            .all(|a| *a == "agent:level-designer#1"),
        "{history:?}"
    );
    assert_eq!(who("task_review"), ["agent:qa-playtester#1"]);
    mcp.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn m3s_repeated_rejections_block_the_task_for_the_human_only() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    let mcp = mcp(&root).await;
    let id = create(&root, &mcp, "Arena walls").await;
    for attempt in 1..=3 {
        submit(&root, &mcp, &id).await;
        agent_call(
            &root,
            &mcp,
            QA,
            "task_reject",
            json!({"id": id, "comment": format!("walls missing ({attempt})")}),
        )
        .await;
    }
    let task = status(&root, &mcp, &id).await;
    assert_eq!(task["status"], "blocked");
    assert_eq!(task["assignee"], "human:local");

    let agent = agent_try(
        &root,
        &mcp,
        None,
        "task_update",
        json!({"id": id, "status": "todo"}),
    )
    .await;
    assert!(agent.is_err(), "only a human unblocks");

    let (engine, human) = human(&root).await;
    engine
        .call(human, "task.update", json!({"id": id, "status": "todo"}))
        .await
        .unwrap();
    assert_eq!(status(&root, &mcp, &id).await["status"], "todo");
    let last = actors(&root, &id).pop().unwrap();
    assert_eq!(last, ("task_changed".into(), "human:local".into()));
    mcp.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn m3s_a_hand_edited_task_file_is_logged_as_the_local_human() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    let mcp = mcp(&root).await;
    let id = create(&root, &mcp, "Arena lights").await;
    let path = status(&root, &mcp, &id).await["path"]
        .as_str()
        .unwrap()
        .to_owned();
    let file = root.join(&path);
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(
        &file,
        text.replace("title: Arena lights", "title: Arena lighting"),
    )
    .unwrap();

    assert_eq!(status(&root, &mcp, &id).await["title"], "Arena lighting");
    let edit = StudioJournal::of_project(&root)
        .read(None)
        .unwrap()
        .iter()
        .map(|e| e.to_json())
        .rfind(|e| e["kind"] == "task_changed" && e["task"] == id.as_str())
        .expect("the hand edit is journaled");
    assert_eq!(edit["actor"], "human:local");
    assert_eq!(
        edit["changes"]["title"],
        json!(["Arena lights", "Arena lighting"])
    );
    mcp.shutdown().await;
}
