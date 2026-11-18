//! `concordia headless` and `concordia mcp` (M0 phase 0.7).

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::time::{Duration, Instant};

use common::concordia;
use serde_json::{Value, json};

#[test]
fn headless_stdio_serves_jsonrpc() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = concordia()
        .args(["headless", "--project", dir.path().to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    for (id, method) in [(1, "schema.get"), (2, "scene.info")] {
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {}})
        )
        .unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let response: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["id"], id, "{response}");
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn headless_ws_writes_connection_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = concordia()
        .args([
            "headless",
            "--ws",
            "--port",
            "0",
            "--project",
            dir.path().to_str().unwrap(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let file = dir.path().join(".concordia/headless.json");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !file.is_file() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let info: Option<Value> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    child.kill().unwrap();
    child.wait().unwrap();
    let info = info.expect("connection file written");
    assert!(info["url"].as_str().unwrap().starts_with("ws://127.0.0.1:"));
    assert_eq!(info["token"].as_str().unwrap().len(), 32);
}

#[test]
fn mcp_subcommand_serves_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = concordia()
        .args(["mcp", "--project", dir.path().to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let initialize = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}});
    writeln!(stdin, "{initialize}").unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    drop(stdin);
    let response: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(response["result"]["serverInfo"]["name"], "concordia");
    assert!(child.wait().unwrap().success());
}
