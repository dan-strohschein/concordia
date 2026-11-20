//! `concordia mcp` shares the project's engine (M3 phase 3.5b, ADR-0082): it attaches to a live
//! engine from `.concordia/headless.json`, or hosts one and publishes that file.

mod common;

use std::path::Path;
use std::time::Duration;

use common::{call, connect, connect_with, options, text};
use concordia_api::EngineConfig;
use concordia_api::transport::connection::{ConnectionInfo, HostKind};
use concordia_app::SharedEngine;
use concordia_core::id::SessionId;
use rmcp::model::{ClientConfig, ReadResourceRequestParams, ResourceContents};
use serde_json::{Value, json};

/// The project's engine, hosted by this test as if it were the editor.
async fn editor_host(dir: &Path) -> SharedEngine {
    let project = concordia_app::Project::load(dir).unwrap();
    let mut config = EngineConfig::new(dir);
    config.modules = project.modules().unwrap();
    let engine = SharedEngine::attach_or_host(config, HostKind::Editor)
        .await
        .unwrap();
    assert!(engine.is_host());
    engine
}

async fn human(engine: &SharedEngine) -> SessionId {
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

async fn host_history(engine: &SharedEngine) -> Vec<Value> {
    let session = human(engine).await;
    let history = engine
        .link()
        .call(session, "txn.history", json!({}))
        .await
        .unwrap();
    history["entries"].as_array().unwrap().clone()
}

fn ok(result: &rmcp::model::CallToolResult) {
    assert_ne!(result.is_error, Some(true), "{}", text(result));
}

#[tokio::test]
async fn mcp_attaches_to_live_engine_from_connection_file() {
    let dir = tempfile::tempdir().unwrap();
    let host = editor_host(dir.path()).await;
    let client = connect(&dir).await;
    ok(&call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await);
    ok(&call(
        &client,
        "entity_spawn",
        json!({"components": {"Name": "Wall"}}),
    )
    .await);

    let entries = host_history(&host).await;
    assert_eq!(
        entries[0]["actor"], "agent:claude-code",
        "the edit landed in the editor's engine: {entries:?}"
    );
    assert_eq!(
        &ConnectionInfo::read(dir.path()).unwrap().unwrap(),
        host.info(),
        "an attached server publishes nothing"
    );
}

#[tokio::test]
async fn mcp_hosts_and_publishes_connection_file_when_none_live() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    let file = ConnectionInfo::read(dir.path())
        .unwrap()
        .expect("published");
    assert_eq!((file.host, file.pid), (HostKind::Mcp, std::process::id()));
    ok(&call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await);

    // The editor, started later, attaches to the MCP server's engine and sees its scene.
    let editor = SharedEngine::attach_or_host(EngineConfig::new(dir.path()), HostKind::Editor)
        .await
        .unwrap();
    assert!(!editor.is_host());
    let session = human(&editor).await;
    let info = editor
        .link()
        .call(session, "scene.info", json!({}))
        .await
        .unwrap();
    assert_eq!(info["path"], "scenes/a.scene.ron");
}

#[tokio::test]
async fn stale_connection_file_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let mut exited = std::process::Command::new("true").spawn().unwrap();
    let dead = exited.id();
    exited.wait().unwrap();
    ConnectionInfo {
        url: "ws://127.0.0.1:9/".into(),
        token: "stale".into(),
        pid: dead,
        version: concordia_core::version().into(),
        host: HostKind::Editor,
    }
    .write(dir.path())
    .unwrap();

    let client = connect(&dir).await;
    ok(&call(&client, "schema_get", json!({})).await);
    let file = ConnectionInfo::read(dir.path()).unwrap().unwrap();
    assert_eq!(file.host, HostKind::Mcp);
    assert_ne!(file.token, "stale");
    assert!(
        SharedEngine::attach_live(dir.path(), &[])
            .await
            .unwrap()
            .is_some(),
        "the new file works"
    );
}

#[tokio::test]
async fn attached_mcp_reports_host_exit_then_reattaches_or_hosts() {
    let dir = tempfile::tempdir().unwrap();
    let host = editor_host(dir.path()).await;
    let client = connect(&dir).await;
    ok(&call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await);
    ok(&call(&client, "scene_save", json!({})).await);
    let pid = host.info().pid;
    drop(host);
    tokio::time::sleep(Duration::from_millis(100)).await;

    let lost = tokio::time::timeout(
        Duration::from_secs(5),
        call(&client, "scene_info", json!({})),
    )
    .await
    .expect("no hang");
    assert_eq!(lost.is_error, Some(true));
    assert!(text(&lost).contains("EngineExited"), "{}", text(&lost));
    assert!(
        text(&lost).contains(&format!("pid {pid}")),
        "{}",
        text(&lost)
    );

    // The next call reconnects: nobody else is live, so this server hosts a fresh engine.
    let next = call(&client, "scene_info", json!({})).await;
    assert!(text(&next).contains("NoSceneOpen"), "{}", text(&next));
    let file = ConnectionInfo::read(dir.path()).unwrap().unwrap();
    assert_eq!(file.host, HostKind::Mcp);
    ok(&call(&client, "scene_open", json!({"path": "scenes/a.scene.ron"})).await);
}

#[tokio::test]
async fn scene_resource_is_served_when_attached() {
    let dir = tempfile::tempdir().unwrap();
    let host = editor_host(dir.path()).await;
    let client = connect(&dir).await;
    ok(&call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await);
    ok(&call(
        &client,
        "entity_spawn",
        json!({"components": {"Name": "Tower"}}),
    )
    .await);
    let read = client
        .read_resource(ReadResourceRequestParams::new("concordia://scene/current"))
        .await
        .unwrap();
    let ResourceContents::TextResourceContents { text, .. } = &read.contents[0] else {
        panic!("text contents")
    };
    assert!(text.contains("Tower"), "{text}");
    let session = human(&host).await;
    let info = host
        .link()
        .call(session, "scene.info", json!({}))
        .await
        .unwrap();
    assert_eq!(
        info["entity_count"], 1,
        "the resource reads the host's scene: {info}"
    );
}

/// A Studio project with the default team (level-designer ×2).
fn studio_project(dir: &Path) {
    concordia_studio::init(dir, Path::new("studio")).unwrap();
    std::fs::write(
        dir.join("concordia.toml"),
        "[physics]\n\n[assets]\n\n[render]\n\n[scripts]\n\n[replay]\n\n[studio]\n",
    )
    .unwrap();
}

fn stamped(mut args: Value, role: &str, agent: &str) -> Value {
    args["_studio"] = json!({"role": role, "agent": agent, "tool_use_id": "t"});
    args
}

#[tokio::test]
async fn studio_sessions_and_attribution_work_when_attached() {
    let dir = tempfile::tempdir().unwrap();
    studio_project(dir.path());
    let host = editor_host(dir.path()).await;
    let mut opts = options(&dir);
    opts.modules = concordia_app::Project::load(dir.path())
        .unwrap()
        .modules()
        .unwrap();
    opts.studio = Some("studio".into());
    let client = connect_with(opts, ClientConfig::default()).await;

    let scene = json!({"path": "scenes/a.scene.ron", "name": "A"});
    ok(&call(
        &client,
        "scene_new",
        stamped(scene, "level-designer", "s/1"),
    )
    .await);
    let begin = json!({"label": "walls"});
    let begun = call(
        &client,
        "txn_begin",
        stamped(begin, "level-designer", "s/1"),
    )
    .await;
    ok(&begun);
    let txn = begun.structured_content.unwrap()["txn"].clone();
    let spawn = json!({"components": {"Name": "Wall"}, "actor": "agent:director"});
    ok(&call(
        &client,
        "entity_spawn",
        stamped(spawn, "level-designer", "s/1"),
    )
    .await);
    let other = json!({"components": {"Name": "X"}});
    let conflict = call(
        &client,
        "entity_spawn",
        stamped(other, "level-designer", "s/2"),
    )
    .await;
    assert!(
        text(&conflict).contains("TxnConflict")
            && text(&conflict).contains("agent:level-designer#1"),
        "each agent has its own host session: {}",
        text(&conflict)
    );
    let commit = json!({"txn": txn});
    ok(&call(
        &client,
        "txn_commit",
        stamped(commit, "level-designer", "s/1"),
    )
    .await);
    let unstamped = call(
        &client,
        "entity_spawn",
        json!({"components": {"Name": "Y"}}),
    )
    .await;
    assert!(
        text(&unstamped).contains("NotPermitted"),
        "unstamped calls stay read-only: {}",
        text(&unstamped)
    );

    let entries = host_history(&host).await;
    assert_eq!(
        entries[0]["actor"], "agent:level-designer#1",
        "attributed to the role instance in the host's log: {entries:?}"
    );
}
