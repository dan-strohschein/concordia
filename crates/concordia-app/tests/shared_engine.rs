//! One engine per project (M3 phase 3.5b, ADR-0082): attach to the live engine, or host and publish.

use std::time::Duration;

use concordia_api::EngineConfig;
use concordia_api::transport::connection::{ConnectionInfo, HostKind};
use concordia_app::{AppError, SharedEngine};
use serde_json::json;

fn config(dir: &tempfile::TempDir) -> EngineConfig {
    let mut config = EngineConfig::new(dir.path());
    config.threads = std::num::NonZeroUsize::new(1).unwrap();
    config
}

async fn start(dir: &tempfile::TempDir, host: HostKind) -> SharedEngine {
    SharedEngine::attach_or_host(config(dir), host)
        .await
        .unwrap()
}

/// The pid of a process that has exited.
fn dead_pid() -> u32 {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

async fn new_scene(engine: &SharedEngine, actor: &str) -> concordia_core::id::SessionId {
    let link = engine.link();
    let session = link.open_session().await.unwrap();
    link.call(
        session,
        "session.open",
        json!({"client": "t", "actor": actor}),
    )
    .await
    .unwrap();
    session
}

#[tokio::test]
async fn first_start_hosts_and_publishes_the_connection_file() {
    let dir = tempfile::tempdir().unwrap();
    let engine = start(&dir, HostKind::Mcp).await;
    assert!(engine.is_host());
    assert!(engine.handle().is_some());
    let file = ConnectionInfo::read(dir.path())
        .unwrap()
        .expect("published");
    assert_eq!(&file, engine.info());
    assert_eq!((file.pid, file.host), (std::process::id(), HostKind::Mcp));
}

#[tokio::test]
async fn second_start_attaches_to_the_live_engine() {
    let dir = tempfile::tempdir().unwrap();
    let host = start(&dir, HostKind::Editor).await;
    let attached = start(&dir, HostKind::Mcp).await;
    assert!(!attached.is_host());
    assert!(attached.handle().is_none());
    assert_eq!(attached.info(), host.info(), "the file is not rewritten");

    let s = new_scene(&attached, "agent:a#1").await;
    attached
        .link()
        .call(
            s,
            "scene.new",
            json!({"path": "scenes/s.scene.ron", "name": "S"}),
        )
        .await
        .unwrap();
    let h = new_scene(&host, "human:local").await;
    let info = host.link().call(h, "scene.info", json!({})).await.unwrap();
    assert_eq!(
        info["path"], "scenes/s.scene.ron",
        "one engine: the host sees the attached client's scene"
    );
}

#[tokio::test]
async fn stale_connection_file_with_a_dead_pid_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let stale = ConnectionInfo {
        url: "ws://127.0.0.1:9/".into(),
        token: "old".into(),
        pid: dead_pid(),
        version: concordia_core::version().into(),
        host: HostKind::Editor,
    };
    stale.write(dir.path()).unwrap();
    let engine = start(&dir, HostKind::Mcp).await;
    assert!(engine.is_host());
    let file = ConnectionInfo::read(dir.path()).unwrap().unwrap();
    assert_ne!(file.token, "old");
    assert_eq!(file.pid, std::process::id());
}

#[tokio::test]
async fn connection_file_with_a_rejected_token_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let live = start(&dir, HostKind::Headless).await;
    // A file whose pid is alive (this process) but whose token the server refuses.
    let mut forged = live.info().clone();
    forged.token = "not-the-token".into();
    let other = tempfile::tempdir().unwrap();
    forged.write(other.path()).unwrap();
    let engine = start(&other, HostKind::Mcp).await;
    assert!(engine.is_host(), "a refused token means stale");
    let file = ConnectionInfo::read(other.path()).unwrap().unwrap();
    assert_ne!(file.token, "not-the-token");
    assert_ne!(file.url, live.info().url);
}

#[tokio::test]
async fn concurrent_starts_share_one_engine() {
    let dir = tempfile::tempdir().unwrap();
    let starts = (0..4).map(|_| SharedEngine::attach_or_host(config(&dir), HostKind::Mcp));
    let engines: Vec<SharedEngine> = futures::future::join_all(starts)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    assert_eq!(engines.iter().filter(|e| e.is_host()).count(), 1);
    let token = &engines[0].info().token;
    assert!(engines.iter().all(|e| &e.info().token == token));
}

#[tokio::test]
async fn a_live_engine_of_another_version_is_not_attached() {
    let dir = tempfile::tempdir().unwrap();
    let host = start(&dir, HostKind::Editor).await;
    let mut older = host.info().clone();
    older.version = "0.0.1-old".into();
    older.write(dir.path()).unwrap();
    let err = SharedEngine::attach_or_host(config(&dir), HostKind::Mcp)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, AppError::VersionMismatch { theirs, .. } if theirs == "0.0.1-old"),
        "{err}"
    );
    assert!(err.to_string().contains("the editor"), "{err}");
    assert_eq!(
        ConnectionInfo::read(dir.path()).unwrap().unwrap(),
        older,
        "a live engine's file is left alone"
    );
}

#[tokio::test]
async fn dropping_the_host_removes_its_connection_file() {
    let dir = tempfile::tempdir().unwrap();
    let engine = start(&dir, HostKind::Headless).await;
    assert!(ConnectionInfo::path(dir.path()).exists());
    drop(engine);
    assert!(!ConnectionInfo::path(dir.path()).exists());
}

#[tokio::test]
async fn attach_live_finds_only_a_live_engine() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        SharedEngine::attach_live(dir.path(), &[])
            .await
            .unwrap()
            .is_none()
    );
    let host = start(&dir, HostKind::Editor).await;
    let remote = SharedEngine::attach_live(dir.path(), &[])
        .await
        .unwrap()
        .expect("live");
    assert_eq!(remote.info(), host.info());
    drop(remote);
    drop(host);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        SharedEngine::attach_live(dir.path(), &[])
            .await
            .unwrap()
            .is_none()
    );
}
