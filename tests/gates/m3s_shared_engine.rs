//! Studio checkpoint gate (ADR-0049, ADR-0082): one engine per project. With the editor hosting a
//! Studio project, Claude Code's `concordia mcp` attaches to it, so agents' scene edits and task
//! changes land in the editor's command log and Studio journal under their own actors. When the
//! editor exits, the next MCP call reports `EngineExited`, and the one after hosts a fresh engine.

use std::path::Path;
use std::time::Duration;

use concordia_api::EngineConfig;
use concordia_api::transport::connection::{ConnectionInfo, HostKind};
use concordia_app::SharedEngine;
use concordia_core::id::SessionId;
use concordia_tests::studio_gate::{agent_call, agent_try, mcp, new_project};
use serde_json::{Value, json};

const LD: Option<&str> = Some("level-designer");

/// The project's engine, hosted in this process as the editor hosts it.
async fn editor_host(root: &Path) -> SharedEngine {
    let mut config = EngineConfig::new(root);
    config.modules = concordia_app::Project::load(root)
        .unwrap()
        .modules()
        .unwrap();
    let engine = SharedEngine::attach_or_host(config, HostKind::Editor)
        .await
        .unwrap();
    assert!(engine.is_host());
    engine
}

/// The editor's own session (`human:local`).
async fn editor_session(engine: &SharedEngine) -> SessionId {
    let session = engine.link().open_session().await.unwrap();
    engine
        .link()
        .call(
            session,
            "session.open",
            json!({"client": "editor", "actor": "human:local"}),
        )
        .await
        .unwrap();
    session
}

async fn editor_call(
    engine: &SharedEngine,
    session: SessionId,
    method: &str,
    params: Value,
) -> Value {
    engine
        .link()
        .call(session, method, params)
        .await
        .unwrap_or_else(|e| panic!("{method}: {}", e.message))
}

#[tokio::test(flavor = "multi_thread")]
async fn m3s_agents_and_the_editor_share_one_engine() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    let editor = editor_host(&root).await;
    let session = editor_session(&editor).await;
    editor_call(
        &editor,
        session,
        "scene.open",
        json!({"path": "scenes/main.scene.ron"}),
    )
    .await;

    let mcp = mcp(&root).await;
    let file = ConnectionInfo::read(&root).unwrap().unwrap();
    assert_eq!(
        (file.host, file.pid),
        (HostKind::Editor, std::process::id()),
        "MCP attached, not hosted"
    );

    // The agent sees the scene the editor opened, and its edit lands in the editor's command log.
    let info = agent_call(&root, &mcp, LD, "scene_info", json!({})).await;
    assert_eq!(info["path"], "scenes/main.scene.ron", "{info}");
    agent_call(
        &root,
        &mcp,
        LD,
        "entity_spawn",
        json!({"components": {"Name": "Wall"}}),
    )
    .await;
    let history = editor_call(&editor, session, "txn.history", json!({})).await;
    let last = history["entries"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["actor"], "agent:level-designer#1", "{last}");

    // Task changes too: the editor's Studio journal view shows them with the agents' actors.
    let task = agent_call(
        &root,
        &mcp,
        None,
        "task_create",
        json!({"title": "Arena", "role": "level-designer"}),
    )
    .await;
    let id = task["task"]["id"].as_str().unwrap().to_owned();
    agent_call(&root, &mcp, LD, "task_claim", json!({"id": id})).await;
    let activity = editor_call(&editor, session, "studio.activity", json!({"task": id})).await;
    let actors: Vec<&str> = activity["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["actor"].as_str().unwrap())
        .collect();
    assert_eq!(actors, ["agent:director#1", "agent:level-designer#1"]);

    // The editor exits: the next agent call says so; the one after hosts a fresh engine.
    let pid = file.pid;
    drop(editor);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let lost = tokio::time::timeout(
        Duration::from_secs(5),
        agent_try(&root, &mcp, LD, "task_get", json!({"id": id})),
    )
    .await
    .expect("no hang")
    .unwrap_err();
    assert!(
        lost.text.contains("EngineExited") && lost.text.contains(&format!("pid {pid}")),
        "{}",
        lost.text
    );
    let task = agent_call(&root, &mcp, LD, "task_get", json!({"id": id})).await;
    assert_eq!(
        task["task"]["id"],
        id.as_str(),
        "the board is text, so it survives the host"
    );
    let file = ConnectionInfo::read(&root).unwrap().unwrap();
    assert_eq!(file.host, HostKind::Mcp);
    mcp.shutdown().await;
}
