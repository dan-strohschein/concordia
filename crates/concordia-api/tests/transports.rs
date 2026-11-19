//! stdio and WebSocket transports (M0 phase 0.6).

mod common;

use std::net::SocketAddr;

use concordia_api::transport::connection::{ConnectionInfo, HostKind};
use concordia_api::transport::{stdio, ws};
use concordia_api::{Engine, EngineHandle};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

fn engine() -> (tempfile::TempDir, EngineHandle) {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::new(common::config(&dir)).unwrap();
    (dir, engine.spawn())
}

#[tokio::test]
async fn stdio_transport_handles_ndjson_requests() {
    let (_dir, handle) = engine();
    let (client, server) = tokio::io::duplex(64 * 1024);
    let (server_read, server_write) = tokio::io::split(server);
    let task = tokio::spawn(async move { stdio::serve(&handle, server_read, server_write).await });
    let (client_read, mut client_write) = tokio::io::split(client);
    let mut lines = BufReader::new(client_read).lines();

    for (id, method) in [(1, "session.open"), (2, "schema.get")] {
        let params = if method == "session.open" {
            json!({"client": "t", "actor": "agent:t"})
        } else {
            json!({"name": "Name"})
        };
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        client_write
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        let response: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(response["id"], id, "{response}");
        assert!(response.get("result").is_some(), "{response}");
    }
    client_write
        .write_all(b"\n{\"jsonrpc\":\"2.0\",\"method\":\"scene.info\"}\n")
        .await
        .unwrap();
    client_write.shutdown().await.unwrap();
    assert!(
        lines.next_line().await.unwrap().is_none(),
        "blank lines and notifications produce no output"
    );
    task.await.unwrap().unwrap();
}

async fn ws_server() -> (tempfile::TempDir, ws::WsServer) {
    let (dir, handle) = engine();
    let server = ws::serve(handle, ws::WsConfig::localhost(0)).await.unwrap();
    (dir, server)
}

#[tokio::test]
async fn ws_transport_serves_jsonrpc() {
    let (_dir, server) = ws_server().await;
    let url = format!("{}?token={}", server.url(), server.token);
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let request =
        json!({"jsonrpc": "2.0", "id": 7, "method": "schema.get", "params": {"name": "Transform"}});
    socket
        .send(Message::text(request.to_string()))
        .await
        .unwrap();
    let reply = socket.next().await.unwrap().unwrap();
    let response: Value = serde_json::from_str(reply.to_text().unwrap()).unwrap();
    assert_eq!(response["id"], 7);
    assert_eq!(response["result"]["schema"]["title"], "Transform");
}

#[tokio::test]
async fn ws_rejects_missing_token_and_foreign_origin() {
    let (_dir, server) = ws_server().await;
    let status = |result: Result<_, tokio_tungstenite::tungstenite::Error>| match result {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => response.status().as_u16(),
        Err(other) => panic!("unexpected error {other}"),
        Ok(_) => panic!("connection should be refused"),
    };
    assert_eq!(
        status(
            tokio_tungstenite::connect_async(server.url())
                .await
                .map(|_| ())
        ),
        401
    );
    let wrong = format!("{}?token=wrong", server.url());
    assert_eq!(
        status(tokio_tungstenite::connect_async(wrong).await.map(|_| ())),
        401
    );

    let mut request = format!("{}?token={}", server.url(), server.token)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", "https://evil.example".parse().unwrap());
    assert_eq!(
        status(tokio_tungstenite::connect_async(request).await.map(|_| ())),
        403
    );

    let mut local = format!("{}?token={}", server.url(), server.token)
        .into_client_request()
        .unwrap();
    local
        .headers_mut()
        .insert("Origin", "http://localhost:5173".parse().unwrap());
    assert!(
        tokio_tungstenite::connect_async(local).await.is_ok(),
        "localhost origins are allowed"
    );
}

#[tokio::test]
async fn ws_binds_localhost_only() {
    let (_dir, handle) = engine();
    let mut config = ws::WsConfig::localhost(0);
    config.addr = "0.0.0.0:0".parse::<SocketAddr>().unwrap();
    assert!(ws::serve(handle, config).await.is_err());
}

#[tokio::test]
async fn connection_file_is_private_and_complete() {
    let (dir, server) = ws_server().await;
    // 3.5b: written through `ConnectionInfo` (adds `version` and `host`; ADR-0082).
    let info = ConnectionInfo::for_server(&server, HostKind::Headless);
    let path = info.write(dir.path()).unwrap();
    let info: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(info["url"], server.url());
    assert_eq!(info["token"], server.token);
    assert_eq!(info["pid"], std::process::id());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Sends a request and returns its response, collecting any notifications that arrive first.
async fn request(
    socket: &mut Socket,
    id: u64,
    method: &str,
    params: Value,
    notes: &mut Vec<Value>,
) -> Value {
    let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    socket
        .send(Message::text(message.to_string()))
        .await
        .unwrap();
    loop {
        let reply = socket.next().await.unwrap().unwrap();
        let value: Value = serde_json::from_str(reply.to_text().unwrap()).unwrap();
        match value.get("id") {
            Some(got) if got == id => return value,
            _ => notes.push(value),
        }
    }
}

#[tokio::test]
async fn ws_subscriber_receives_another_clients_edits_as_notifications() {
    let (_dir, server) = ws_server().await;
    let url = format!("{}?token={}", server.url(), server.token);
    let (mut editor, _) = tokio_tungstenite::connect_async(url.clone()).await.unwrap();
    let (mut agent, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let mut notes = Vec::new();
    request(
        &mut editor,
        1,
        "session.open",
        json!({"client": "editor", "actor": "human:local"}),
        &mut notes,
    )
    .await;
    request(
        &mut editor,
        2,
        "scene.new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
        &mut notes,
    )
    .await;
    let sub = request(
        &mut editor,
        3,
        "events.subscribe",
        json!({"topics": ["edit"]}),
        &mut notes,
    )
    .await;
    assert!(sub.get("result").is_some(), "{sub}");
    request(
        &mut agent,
        1,
        "session.open",
        json!({"client": "agent", "actor": "agent:test#1"}),
        &mut notes,
    )
    .await;
    let spawned = request(
        &mut agent,
        2,
        "entity.spawn",
        json!({"components": {"Name": "Crate"}}),
        &mut notes,
    )
    .await;
    let id = spawned["result"]["id"].clone();
    let note = tokio::time::timeout(std::time::Duration::from_secs(2), editor.next())
        .await
        .expect("a notification within 2 s")
        .unwrap()
        .unwrap();
    let note: Value = serde_json::from_str(note.to_text().unwrap()).unwrap();
    assert_eq!(note["method"], "events.event", "{note}");
    assert!(
        note.get("id").is_none(),
        "notifications carry no id: {note}"
    );
    assert_eq!(
        (
            note["params"]["actor"].as_str(),
            &note["params"]["entities"][0]
        ),
        (Some("agent:test#1"), &id)
    );
}

#[tokio::test]
async fn handle_run_executes_on_the_engine_thread_in_order() {
    let (_dir, handle) = engine();
    let session = handle.open_session().await.unwrap();
    handle
        .call(
            session,
            "session.open",
            json!({"client": "t", "actor": "agent:t"}),
        )
        .await
        .unwrap();
    let seen = handle
        .run(Box::new(|engine: &mut Engine| {
            engine.methods().get("schema.get").is_some()
                && std::thread::current().name() == Some("concordia-engine")
        }))
        .await
        .unwrap();
    assert!(seen, "the closure ran with the engine, on its thread");
}
