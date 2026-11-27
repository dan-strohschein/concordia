//! An agent authors a gameplay graph over MCP only (M4 4.10c, J20): reads the node list, writes
//! the graph with `graph_apply`, checks and compiles it, attaches it, steps play, reads the result
//! and traces it.

use crate::common;

use common::{call, connect_with, options, text};
use serde_json::{Value, json};

fn ok(result: &rmcp::model::CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "{}", text(result));
    serde_json::from_str(&text(result)).unwrap()
}

fn id(n: u32) -> String {
    format!("0193b0f2-0000-7000-8000-{n:012x}")
}

#[tokio::test]
async fn agent_authors_graph_over_mcp_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("concordia.toml"),
        "[[scripts.module]]\nkind = \"graph\"\npath = \"graphs/agent.graph.ron\"\n",
    )
    .unwrap();
    let mut options = options(&dir);
    options.modules = concordia_app::Project::load(dir.path())
        .and_then(|p| p.modules())
        .unwrap();
    let client = connect_with(options, Default::default()).await;
    ok(&call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await);

    // The agent picks its nodes from the list.
    let nodes = ok(&call(&client, "graph_list_nodes", json!({})).await)["nodes"].clone();
    let tick = &nodes["event.on_tick"];
    assert!(
        tick["outputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "dt"),
        "{tick}"
    );
    for op in [
        "get.Transform.translation",
        "set.Transform.translation",
        "conv.vec3",
        "math.add",
    ] {
        assert!(nodes.get(op).is_some(), "{op} is offered");
    }

    // "Every tick, move up by dt."
    let edits = json!([
        {"AddNode": {"id": id(1), "op": "event.on_tick", "at": [0.0, 0.0]}},
        {"AddNode": {"id": id(2), "op": "get.Transform.translation", "at": [0.0, 120.0]}},
        {"AddNode": {"id": id(3), "op": "conv.vec3", "at": [200.0, 0.0]}},
        {"AddNode": {"id": id(4), "op": "math.add", "at": [400.0, 60.0]}},
        {"AddNode": {"id": id(5), "op": "set.Transform.translation", "at": [600.0, 0.0]}},
        {"SetValue": {"node": id(3), "pin": "x", "value": {"Float": 0.0}}},
        {"Connect": {"from": id(1), "from_pin": "dt", "to": id(3), "to_pin": "y"}},
        {"SetValue": {"node": id(3), "pin": "z", "value": {"Float": 0.0}}},
        {"Connect": {"from": id(2), "from_pin": "value", "to": id(4), "to_pin": "a"}},
        {"Connect": {"from": id(3), "from_pin": "out", "to": id(4), "to_pin": "b"}},
        {"Connect": {"from": id(4), "from_pin": "out", "to": id(5), "to_pin": "value"}},
        {"ConnectExec": {"from": id(1), "from_pin": "out", "to": id(5), "to_pin": "in"}},
    ]);
    let applied = ok(&call(
        &client,
        "graph_apply",
        json!({"path": "graphs/agent.graph.ron", "kind": "Event", "edits": edits}),
    )
    .await);
    assert_eq!(
        (applied["ok"].clone(), applied["reloaded"].clone()),
        (json!(true), json!(true)),
        "{applied}"
    );
    assert_eq!(
        ok(&call(
            &client,
            "graph_check",
            json!({"path": "graphs/agent.graph.ron"})
        )
        .await)["ok"],
        true
    );
    let compiled = ok(&call(
        &client,
        "graph_compile",
        json!({"path": "graphs/agent.graph.ron"}),
    )
    .await);
    assert_eq!(compiled["vars"], "graph.agent.Vars");
    assert_eq!(compiled["processors"][0]["name"], "graph.agent.tick");

    // Attach it and run.
    let spawned = ok(&call(
        &client,
        "entity_spawn",
        json!({"components": {
            "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
            "graph.agent.Vars": {}
        }}),
    )
    .await);
    let entity = spawned["id"].as_str().unwrap().to_owned();
    ok(&call(&client, "sim_step", json!({"ticks": 60})).await);
    let got = ok(&call(
        &client,
        "component_get",
        json!({"entity": entity, "component": "Transform", "world": "play"}),
    )
    .await);
    let y = got["value"]["translation"][1].as_f64().unwrap();
    assert!((y - 1.0).abs() < 1e-4, "one second of rising: {got}");

    // And see what it did.
    let traced = ok(&call(
        &client,
        "graph_trace",
        json!({"module": "agent", "entity": entity, "ticks": 2}),
    )
    .await);
    let runs = traced["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2, "{traced}");
    assert_eq!(runs[0]["jobs"][0]["executed"], json!([id(5)]));
    let dt = runs[0]["jobs"][0]["pins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["node"] == id(1) && p["pin"] == "dt")
        .unwrap()
        .clone();
    assert!(
        (dt["values"][0].as_f64().unwrap() - 1.0 / 60.0).abs() < 1e-6,
        "{dt}"
    );
    client.cancel().await.ok();
}
