//! M0 gate (layer 1): an agent with no API key creates, edits, undoes, saves,
//! reopens and queries a scene entirely over MCP, against the real binary.

use std::path::Path;

use concordia_api::{methods, params_schema};
use concordia_reflect::TypeRegistry;
use concordia_tests::bins::concordia_mcp_bin;
use concordia_tests::mcp_client::McpClient;
use serde_json::{Value, json};

const SCENE: &str = "scenes/arena.scene.ron";

async fn server(project: &Path, extra: &[&str]) -> McpClient {
    let project = project.to_str().unwrap();
    let mut args = vec![
        "--project",
        project,
        "--actor",
        "agent:gate",
        "--deterministic-ids",
        "7",
    ];
    args.extend_from_slice(extra);
    McpClient::spawn(&concordia_mcp_bin(), &args)
        .await
        .expect("server starts")
}

fn transform(x: f32) -> Value {
    json!({"translation": [x, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]})
}

async fn spawn(client: &McpClient, components: Value) -> String {
    let result = client
        .call("entity_spawn", json!({ "components": components }))
        .await
        .unwrap();
    result["id"].as_str().unwrap().to_owned()
}

async fn translation_x(client: &McpClient, id: &str) -> f64 {
    let value = client
        .call(
            "component_get",
            json!({"entity": id, "component": "Transform"}),
        )
        .await
        .unwrap();
    value["value"]["translation"][0].as_f64().unwrap()
}

/// Steps 1–2: 24 tools with generated schemas; schema resources carry doc comments.
async fn check_tools_and_schemas(client: &McpClient) {
    let tools = client.tools().await;
    let mut names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    let mut expected: Vec<String> = methods()
        .iter()
        .filter(|m| concordia_mcp::is_tool(m))
        .map(|m| m.name.replace('.', "_"))
        .collect();
    expected.sort();
    assert_eq!(names, expected);
    assert_eq!(
        names.len(),
        30,
        "the 24 M0 tools plus M1 1.13's entity.templates/create/duplicate/reparent, M1 1.14's component.add and M1 1.15's txn.undo_to"
    );
    for tool in &tools {
        let method = methods().get(&tool.name.replacen('_', ".", 1)).unwrap();
        let generated = serde_json::to_string(&params_schema(method)).unwrap();
        let served = serde_json::to_string(tool.input_schema.as_ref()).unwrap();
        assert_eq!(
            served, generated,
            "{} input schema is not the generated one",
            tool.name
        );
    }
    let schema: Value =
        serde_json::from_str(&client.read("concordia://schema/component/Transform").await).unwrap();
    assert_eq!(
        schema["description"],
        "Local transform relative to `Parent` (or to the world if there is no parent)."
    );
}

/// Steps 3–10 in the first server process. Returns (player, crates).
async fn build_and_edit(client: &McpClient) -> (String, Vec<String>) {
    client
        .call("scene_new", json!({"path": SCENE, "name": "Arena"}))
        .await
        .unwrap();

    let txn = client
        .call("txn_begin", json!({"label": "build arena"}))
        .await
        .unwrap()["txn"]
        .clone();
    let player = spawn(
        client,
        json!({"Name": "Player", "Transform": transform(0.0)}),
    )
    .await;
    let mut crates = Vec::new();
    for i in 1..=3 {
        let velocity = json!({"linear": [0.0, 0.0, i as f32], "angular": [0.0, 0.5, 0.0]});
        let components = json!({"Name": format!("Crate {i}"), "Transform": transform(i as f32 * -2.0), "Velocity": velocity});
        crates.push(spawn(client, components).await);
    }
    assert_eq!(
        client
            .call("txn_commit", json!({"txn": txn}))
            .await
            .unwrap()["ops"],
        4
    );

    client
        .call(
            "component_set",
            json!({"entity": player, "component": "Transform", "value": transform(5.0)}),
        )
        .await
        .unwrap();
    let typo = client
        .call(
            "component_set",
            json!({"entity": player, "component": "Transfrom", "value": transform(1.0)}),
        )
        .await;
    let error = typo.expect_err("a misspelled component is a tool error");
    assert!(
        error.text.contains("Transform"),
        "the error suggests the right name: {}",
        error.text
    );

    client.call("txn_undo", json!({})).await.unwrap();
    assert_eq!(translation_x(client, &player).await, 0.0);
    client.call("txn_redo", json!({})).await.unwrap();
    assert_eq!(translation_x(client, &player).await, 5.0);

    let crate3 = client
        .call("entity_get", json!({"id": crates[2]}))
        .await
        .unwrap();
    client
        .call("entity_despawn", json!({"id": crates[2]}))
        .await
        .unwrap();
    client.call("txn_undo", json!({})).await.unwrap();
    assert_eq!(
        client
            .call("entity_get", json!({"id": crates[2]}))
            .await
            .unwrap(),
        crate3
    );

    let patch =
        json!({"entity": crates[0], "component": "Transform", "patch": {"scale": [2.0, 2.0, 2.0]}});
    client.call("component_patch", patch).await.unwrap();

    let saved = client.call("scene_save", json!({})).await.unwrap();
    assert_eq!(saved["path"], SCENE);
    (player, crates)
}

fn check_scene_file(project: &Path) {
    let text = std::fs::read_to_string(project.join(SCENE)).expect("scene file exists");
    insta::assert_snapshot!("m0_gate_arena_scene", text);
    let mut registry = TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    assert_eq!(
        concordia_scene::format(&text, &registry).unwrap(),
        text,
        "the saved scene is canonical"
    );
}

/// Step 11 in a fresh server process.
async fn reopen_and_query(project: &Path, player: &str, crates: &[String]) {
    let client = server(project, &[]).await;
    let opened = client
        .call("scene_open", json!({"path": SCENE}))
        .await
        .unwrap();
    assert_eq!(opened["history"], "restored");
    let far = json!({"with": ["Transform"], "where": [{"path": "Transform/translation/0", "op": "gt", "value": 4}]});
    let rows = client.call("query_run", far).await.unwrap()["rows"].clone();
    assert_eq!(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].clone())
            .collect::<Vec<_>>(),
        vec![json!(player)]
    );
    let named = client
        .call("query_run", json!({"with": ["Name"]}))
        .await
        .unwrap()["rows"]
        .clone();
    let ids: Vec<String> = named
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!((ids.len(), &ids), (4, &sorted));

    client.call("txn_undo", json!({})).await.unwrap();
    let crate1 = client
        .call(
            "component_get",
            json!({"entity": crates[0], "component": "Transform"}),
        )
        .await
        .unwrap();
    assert_eq!(
        crate1["value"]["scale"],
        json!([1.0, 1.0, 1.0]),
        "persisted history undoes the patch"
    );
    check_history_and_logs(&client).await;
    client.shutdown().await;
}

/// Step 13: txn_history and log_tail over MCP.
async fn check_history_and_logs(client: &McpClient) {
    let history = client.call("txn_history", json!({})).await.unwrap()["entries"].clone();
    let kinds: Vec<&str> = history
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "commit", "commit", "undo", "redo", "commit", "undo", "commit", "save", "undo"
        ]
    );
    let logs = client.call("log_tail", json!({})).await.unwrap()["records"].clone();
    let messages: Vec<&str> = logs
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["message"].as_str().unwrap())
        .collect();
    assert!(messages.contains(&"scene opened"), "{messages:?}");
}

/// Step 12: the command journal.
fn check_journal(project: &Path) {
    let dir = project.join(".concordia/journal");
    let file = std::fs::read_dir(&dir)
        .unwrap()
        .next()
        .expect("one journal")
        .unwrap()
        .path();
    assert!(file.to_string_lossy().ends_with(".cmdlog.jsonl"));
    let entries: Vec<Value> = std::fs::read_to_string(file)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let summary: Vec<String> = entries
        .iter()
        .map(|e| {
            format!(
                "{}:{}:{}",
                e["kind"].as_str().unwrap(),
                e["label"].as_str().unwrap_or(""),
                e["ops"].as_array().map_or(0, Vec::len)
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            "commit:build arena:4",
            "commit:component.set:1",
            "undo::0",
            "redo::0",
            "commit:entity.despawn:1",
            "undo::0",
            "commit:component.patch:1",
            "save::0",
            "undo::0",
        ]
    );
    assert!(
        entries.iter().all(|e| e["actor"] == "agent:gate"),
        "every entry is attributed to agent:gate"
    );
    insta::assert_json_snapshot!("m0_gate_journal", entries, { "[].ts_ms" => "[ts]", "[].session" => "[session]" });
}

#[tokio::test]
async fn m0_gate_agent_workflow_over_mcp() {
    let project = tempfile::tempdir().unwrap();
    let client = server(project.path(), &[]).await;
    assert_eq!(client.server_name().as_deref(), Some("concordia"));
    check_tools_and_schemas(&client).await;
    let (player, crates) = build_and_edit(&client).await;
    client.shutdown().await;
    check_scene_file(project.path());
    reopen_and_query(project.path(), &player, &crates).await;
    check_journal(project.path());
}

#[tokio::test]
async fn m0_gate_sim_is_deterministic_across_restarts_and_threads() {
    let project = tempfile::tempdir().unwrap();
    let client = server(project.path(), &[]).await;
    build_and_edit(&client).await;
    client.shutdown().await;

    let mut hashes = Vec::new();
    for threads in ["1", "8"] {
        let client = server(project.path(), &["--threads", threads, "--scene", SCENE]).await;
        let step = client
            .call("sim_step", json!({"ticks": 300}))
            .await
            .unwrap();
        hashes.push(step["world_hash"].clone());
        client.shutdown().await;
    }
    assert_eq!(
        hashes[0], hashes[1],
        "same scene, 1 vs 8 threads, separate processes"
    );
}
