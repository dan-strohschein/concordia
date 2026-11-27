//! The binary: stdout carries only protocol frames, and no API key is needed (M0 phase 0.7).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn spawn(dir: &tempfile::TempDir, clear_env: bool) -> std::process::Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_concordia-mcp"));
    command.args(["--project", dir.path().to_str().unwrap()]);
    if clear_env {
        command.env_clear();
        for key in ["PATH", "HOME"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
    }
    command
        .env("CONCORDIA_LOG", "trace")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.spawn().unwrap()
}

/// Runs a legacy (2025-11-25) handshake, lists tools and calls one, then closes stdin.
fn session(child: &mut std::process::Child) -> (Vec<Value>, String) {
    let requests = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "scene_info", "arguments": {}}}),
    ];
    // Drain stderr concurrently: trace logs would otherwise fill the pipe and block the child.
    let mut stderr_pipe = child.stderr.take().unwrap();
    let stderr = std::thread::spawn(move || {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut stderr_pipe, &mut text).unwrap();
        text
    });
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut frames = Vec::new();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
        if request.get("id").is_some() {
            let mut line = String::new();
            stdout.read_line(&mut line).unwrap();
            frames.push(
                serde_json::from_str(&line)
                    .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line:?}")),
            );
        }
    }
    drop(stdin);
    let mut rest = String::new();
    std::io::Read::read_to_string(&mut stdout, &mut rest).unwrap();
    for line in rest.lines().filter(|l| !l.trim().is_empty()) {
        frames.push(
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("stdout line is not JSON ({e}): {line:?}")),
        );
    }
    let stderr = stderr.join().unwrap();
    assert!(
        child.wait().unwrap().success(),
        "server exits cleanly at EOF; stderr:\n{stderr}"
    );
    (frames, stderr)
}

#[test]
fn stdout_contains_only_jsonrpc_frames() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = spawn(&dir, false);
    let (frames, stderr) = session(&mut child);
    assert!(frames.iter().all(|f| f["jsonrpc"] == "2.0"), "{frames:?}");
    assert!(!stderr.is_empty(), "trace logs go to stderr");
}

#[test]
fn runs_with_empty_environment() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = spawn(&dir, true);
    let (frames, _) = session(&mut child);
    assert_eq!(frames[0]["result"]["serverInfo"]["name"], "concordia");
    // 24 core tools from M0, entity.templates/create/duplicate/reparent (M1 1.13), component.add (M1 1.14), txn.undo_to (M1 1.15), scene.text (M3 3.5b).
    assert_eq!(frames[1]["result"]["tools"].as_array().unwrap().len(), 35);
    assert_eq!(
        frames[2]["result"]["isError"], true,
        "scene_info without a scene is a tool error: {}",
        frames[2]
    );
}

#[test]
fn binary_serves_tools_of_the_project_modules() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("concordia.toml"),
        "[input]\nmap = \"default.input.ron\"\n",
    )
    .unwrap();
    let map = include_str!("../../../examples/m1-physics-stack/input/default.input.ron");
    std::fs::write(dir.path().join("default.input.ron"), map).unwrap();
    let mut child = spawn(&dir, false);
    let (frames, _) = session(&mut child);
    let tools = frames[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 36, "35 core tools plus the module's");
    assert!(
        tools.iter().any(|t| t["name"] == "input_inject"),
        "{tools:?}"
    );
}

#[test]
fn binary_serves_script_reload_when_scripts_are_configured() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), "[scripts]\n").unwrap();
    let mut child = spawn(&dir, false);
    let (frames, _) = session(&mut child);
    let tools = frames[1]["result"]["tools"].as_array().unwrap();
    // 35 core tools, script_reload and the five graph tools (M4 4.10c).
    assert_eq!(
        tools.len(),
        41,
        "35 core tools plus the scripts module's six"
    );
    for name in [
        "graph_list_nodes",
        "graph_check",
        "graph_compile",
        "graph_apply",
        "graph_trace",
    ] {
        assert!(tools.iter().any(|t| t["name"] == name), "{name}: {tools:?}");
    }
    let reload = tools
        .iter()
        .find(|t| t["name"] == "script_reload")
        .unwrap_or_else(|| panic!("{tools:?}"));
    assert!(
        reload["description"]
            .as_str()
            .unwrap()
            .contains("never builds"),
        "{reload}"
    );
}

#[test]
fn binary_serves_replay_tools_when_replay_is_configured() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), "[replay]\n").unwrap();
    let mut child = spawn(&dir, false);
    let (frames, _) = session(&mut child);
    let tools = frames[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(
        tools.len(),
        38,
        "35 core tools plus replay_record, replay_run and assert_eval (M3 3.2b)"
    );
    for name in ["replay_record", "replay_run", "assert_eval"] {
        assert!(tools.iter().any(|t| t["name"] == name), "{name}: {tools:?}");
    }
}
