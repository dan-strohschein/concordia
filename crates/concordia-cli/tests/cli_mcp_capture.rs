//! `capture_screenshot` end to end: `concordia mcp` on the example project returns MCP image
//! content (M1 phase 1.5). Skips without a GPU unless CONCORDIA_REQUIRE_GPU=1.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;

use common::concordia;
use serde_json::{Value, json};

/// Reads lines until the response with `id`.
fn response(stdout: &mut impl BufRead, id: u64) -> Value {
    loop {
        let mut line = String::new();
        assert!(
            stdout.read_line(&mut line).unwrap() > 0,
            "server closed before answering {id}"
        );
        let message: Value = serde_json::from_str(&line).unwrap();
        if message["id"] == id {
            return message;
        }
    }
}

#[test]
fn mcp_capture_screenshot_returns_image_content() {
    if let Err(concordia_rhi::RhiError::NoDevice { reason }) = concordia_rhi_metal::new_headless() {
        assert!(
            !concordia_testkit::gpu_required(),
            "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
        );
        return;
    }
    let project = format!(
        "{}/../../examples/m1-physics-stack",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut child = concordia()
        .args(["mcp", "--project", &project])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut send = |message: Value| writeln!(stdin, "{message}").unwrap();
    send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}}),
    );
    response(&mut stdout, 1);
    send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    send(
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "scene_open", "arguments": {"path": "scenes/stack.scene.ron"}}}),
    );
    assert_ne!(response(&mut stdout, 2)["result"]["isError"], true);
    send(
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "capture_screenshot", "arguments": {"width": 64, "height": 36}}}),
    );
    let result = response(&mut stdout, 3)["result"].clone();
    drop(send);
    let _ = child.kill();
    let _ = child.wait();
    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(result["content"][0]["type"], "image", "{result}");
    assert_eq!(result["content"][0]["mimeType"], "image/png");
    let meta: Value = serde_json::from_str(result["content"][1]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        (
            meta["width"].as_u64(),
            meta["height"].as_u64(),
            meta["sim_tick"].as_u64()
        ),
        (Some(64), Some(36), Some(0))
    );
}
