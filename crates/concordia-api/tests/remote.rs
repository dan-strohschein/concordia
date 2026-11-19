//! The shared engine's client side (M3 phase 3.5b, ADR-0082): the connection file and
//! `RemoteEngine`, an `EngineLink` over WebSocket.

mod common;

use std::sync::Arc;
use std::time::Duration;

use concordia_api::transport::connection::{ConnectionInfo, HostKind};
use concordia_api::transport::remote::RemoteEngine;
use concordia_api::transport::ws;
use concordia_api::{ApiErrorKind, Engine, EngineHandle, EngineLink, MethodRegistry};
use serde_json::{Value, json};

struct Host {
    dir: tempfile::TempDir,
    handle: EngineHandle,
    server: ws::WsServer,
    info: ConnectionInfo,
}

async fn host() -> Host {
    let dir = tempfile::tempdir().unwrap();
    let handle = Engine::new(common::config(&dir)).unwrap().spawn();
    let server = ws::serve(handle.clone(), ws::WsConfig::localhost(0))
        .await
        .unwrap();
    let info = ConnectionInfo::for_server(&server, HostKind::Headless);
    Host {
        dir,
        handle,
        server,
        info,
    }
}

fn core() -> Arc<MethodRegistry> {
    Arc::new(MethodRegistry::with_core(&[]).unwrap())
}

async fn session(remote: &RemoteEngine, actor: &str) -> concordia_core::id::SessionId {
    let session = remote.open_session().await.unwrap();
    remote
        .call(
            session,
            "session.open",
            json!({"client": "remote-test", "actor": actor}),
        )
        .await
        .unwrap();
    session
}

#[tokio::test]
async fn connection_file_round_trips_and_names_its_host() {
    let host = host().await;
    let path = host.info.write(host.dir.path()).unwrap();
    assert_eq!(path, ConnectionInfo::path(host.dir.path()));
    let read = ConnectionInfo::read(host.dir.path()).unwrap().unwrap();
    assert_eq!(read, host.info);
    assert_eq!(read.pid, std::process::id());
    assert_eq!(read.version, concordia_core::version());
    assert_eq!(read.host, HostKind::Headless);
    assert_eq!(
        read.connect_url(),
        format!("{}?token={}", host.server.url(), host.server.token)
    );
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(raw["host"], "headless", "{raw}");
}

#[test]
fn a_missing_or_garbled_connection_file_reads_as_none() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(ConnectionInfo::read(dir.path()).unwrap(), None);
    let path = ConnectionInfo::path(dir.path());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{\"url\": 3").unwrap();
    assert_eq!(ConnectionInfo::read(dir.path()).unwrap(), None);
}

#[tokio::test]
async fn remove_if_current_keeps_a_newer_hosts_file() {
    let host = host().await;
    host.info.write(host.dir.path()).unwrap();
    let mut newer = host.info.clone();
    newer.token = "a-newer-host".into();
    newer.write(host.dir.path()).unwrap();
    assert!(!host.info.remove_if_current(host.dir.path()).unwrap());
    assert!(ConnectionInfo::path(host.dir.path()).exists());
    assert!(newer.remove_if_current(host.dir.path()).unwrap());
    assert!(!ConnectionInfo::path(host.dir.path()).exists());
    assert!(
        !newer.remove_if_current(host.dir.path()).unwrap(),
        "already gone"
    );
}

#[tokio::test]
async fn connect_fails_for_a_wrong_token_or_a_closed_port() {
    let host = host().await;
    let mut wrong = host.info.clone();
    wrong.token = "not-the-token".into();
    let err = RemoteEngine::connect(&wrong, core()).await.unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::EngineExited, "{err:?}");
    assert!(err.message.contains("token"), "{}", err.message);

    drop(host.server);
    let err = RemoteEngine::connect(&host.info, core()).await.unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::EngineExited, "{err:?}");
}

#[tokio::test]
async fn remote_sessions_are_separate_host_sessions() {
    let host = host().await;
    let remote = RemoteEngine::connect(&host.info, core()).await.unwrap();
    assert!(remote.methods().get("scene.text").is_some());
    let (a, b) = (
        session(&remote, "agent:a#1").await,
        session(&remote, "agent:b#1").await,
    );
    remote
        .call(
            a,
            "scene.new",
            json!({"path": "scenes/r.scene.ron", "name": "R"}),
        )
        .await
        .unwrap();
    let txn = remote
        .call(a, "txn.begin", json!({"label": "a's edit"}))
        .await
        .unwrap();
    remote
        .call(a, "entity.spawn", json!({"components": {"Name": "A"}}))
        .await
        .unwrap();
    let conflict = remote
        .call(b, "entity.spawn", json!({"components": {"Name": "B"}}))
        .await
        .unwrap_err();
    assert_eq!(conflict.kind, ApiErrorKind::TxnConflict, "{conflict:?}");
    assert!(
        !conflict.data.suggestions.is_empty(),
        "errors keep their data"
    );
    remote
        .call(a, "txn.commit", json!({"txn": txn["txn"]}))
        .await
        .unwrap();

    let local = host.handle.open_session().await.unwrap();
    let history = host
        .handle
        .call(local, "txn.history", json!({}))
        .await
        .unwrap();
    assert_eq!(history["entries"][0]["actor"], "agent:a#1", "{history}");
}

#[tokio::test]
async fn host_exit_fails_pending_and_later_calls_with_engine_exited() {
    let host = host().await;
    let remote = RemoteEngine::connect(&host.info, core()).await.unwrap();
    let s = session(&remote, "agent:a#1").await;
    assert!(!remote.exited());

    // Block the engine thread so the next call stays pending, then stop the host.
    let blocked = host.handle.clone();
    tokio::spawn(async move {
        blocked
            .run(|_| std::thread::sleep(Duration::from_secs(3)))
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let pending = {
        let remote = remote.clone();
        tokio::spawn(async move { remote.call(s, "scene.info", json!({})).await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(host.server);

    let err = tokio::time::timeout(Duration::from_secs(2), pending)
        .await
        .expect("a pending call fails promptly when the host exits, never hangs")
        .unwrap()
        .unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::EngineExited, "{err:?}");
    assert!(
        err.message.contains(&host.info.pid.to_string()),
        "names the host's pid: {}",
        err.message
    );
    assert!(remote.exited());
    let later = tokio::time::timeout(
        Duration::from_secs(1),
        remote.call(s, "scene.info", json!({})),
    )
    .await
    .expect("later calls fail at once")
    .unwrap_err();
    assert_eq!(later.kind, ApiErrorKind::EngineExited);
    let opened = tokio::time::timeout(Duration::from_secs(1), remote.open_session())
        .await
        .expect("no hang")
        .unwrap_err();
    assert_eq!(opened.kind, ApiErrorKind::EngineExited);
}

#[tokio::test]
async fn an_idle_client_notices_the_host_exit() {
    let host = host().await;
    let remote = RemoteEngine::connect(&host.info, core()).await.unwrap();
    drop(host.server);
    let start = std::time::Instant::now();
    while !remote.exited() {
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "exit noticed without a call"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
