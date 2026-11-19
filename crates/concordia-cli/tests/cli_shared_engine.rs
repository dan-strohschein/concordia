//! The CLI and the project's shared engine (M3 phase 3.5b, ADR-0082).

mod common;

use std::time::Duration;

use common::{concordia, run, stderr};
use concordia_api::EngineConfig;
use concordia_api::transport::connection::HostKind;
use concordia_app::SharedEngine;
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

/// A new Studio project, with its engine hosted by this test as if it were the editor.
async fn hosted_project(dir: &std::path::Path) -> (std::path::PathBuf, SharedEngine) {
    let output = run(&["new", "g"], dir);
    assert!(output.status.success(), "{}", stderr(&output));
    let root = dir.join("g");
    let mut config = EngineConfig::new(&root);
    config.modules = concordia_app::Project::load(&root)
        .unwrap()
        .modules()
        .unwrap();
    let engine = SharedEngine::attach_or_host(config, HostKind::Editor)
        .await
        .unwrap();
    (root, engine)
}

/// Runs the CLI off the async runtime, so the hosted engine keeps serving meanwhile.
async fn cli(args: &[&str], cwd: &std::path::Path) -> std::process::Output {
    let (args, cwd): (Vec<String>, _) = (
        args.iter().map(|a| (*a).to_owned()).collect(),
        cwd.to_owned(),
    );
    tokio::task::spawn_blocking(move || concordia().args(&args).current_dir(cwd).output().unwrap())
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn headless_ws_refuses_while_an_engine_is_live() {
    let dir = tempfile::tempdir().unwrap();
    let (root, engine) = hosted_project(dir.path()).await;
    let mut child = concordia()
        .args(["headless", "--ws", "--project", "."])
        .current_dir(&root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            panic!("a second engine started and kept running");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success(), "a second engine must not start");
    let err = stderr(&output);
    assert!(
        err.contains("already running") && err.contains(&engine.info().pid.to_string()),
        "{err}"
    );
    assert!(err.contains("the editor"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn task_commands_attach_to_a_live_engine() {
    let dir = tempfile::tempdir().unwrap();
    let (root, engine) = hosted_project(dir.path()).await;
    let (mut socket, _) = tokio_tungstenite::connect_async(engine.info().connect_url())
        .await
        .unwrap();
    for (id, method, params) in [
        (
            1,
            "session.open",
            json!({"client": "editor", "actor": "human:local"}),
        ),
        (2, "events.subscribe", json!({"topics": ["studio"]})),
    ] {
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        socket
            .send(Message::text(request.to_string()))
            .await
            .unwrap();
        let reply: Value =
            serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert!(reply.get("result").is_some(), "{reply}");
    }

    let output = cli(
        &[
            "task",
            "create",
            "--title",
            "Arena",
            "--role",
            "level-designer",
        ],
        &root,
    )
    .await;
    assert!(output.status.success(), "{}", stderr(&output));

    // Only the host's own engine publishes: the event arrives without any further call.
    let note = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("the live engine ran the CLI's call")
        .unwrap()
        .unwrap();
    let note: Value = serde_json::from_str(note.to_text().unwrap()).unwrap();
    assert_eq!(note["params"]["topic"], "studio", "{note}");
    assert!(note.to_string().contains("T-0001"), "{note}");
}
