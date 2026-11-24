//! Replay tools over MCP (M2 phase 2.8; assert_eval, M3 3.2b): generated schemas, and an agent
//! recording, replaying and asserting over a session.

mod common;

use std::sync::Arc;

use common::{call, connect_with, options, text};
use concordia_api::{Engine, EngineConfig, params_schema, result_schema};
use serde_json::{Value, json};

const MAP: &str = include_str!("../../../examples/m1-physics-stack/input/default.input.ron");
const PATH: &str = "tests/replays/agent.replay.ron";

fn json_of(result: &rmcp::model::CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "{}", text(result));
    serde_json::from_str(&text(result)).unwrap()
}

#[test]
fn replay_methods_have_generated_schemas_and_mcp_tools() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = EngineConfig::new(dir.path());
    config.modules = vec![Arc::new(concordia_replay::ReplayModule::new())];
    let engine = Engine::new(config).unwrap();
    for name in ["replay.record", "replay.run", "assert.eval"] {
        let method = engine
            .methods()
            .get(name)
            .unwrap_or_else(|| panic!("{name} missing"));
        assert!(concordia_mcp::is_tool(method), "{name} is an MCP tool");
        assert!(!method.mutates, "{name} never changes the scene");
        assert_eq!(params_schema(method)["type"], "object", "{name}");
        assert_eq!(result_schema(method)["type"], "object", "{name}");
        assert!(method.docs.len() > 20, "{name} is described");
    }
}

#[tokio::test]
async fn replay_record_and_run_over_mcp() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("concordia.toml"),
        "[input]\nmap = \"default.input.ron\"\n\n[replay]\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("default.input.ron"), MAP).unwrap();
    let mut options = options(&dir);
    options.modules = concordia_app::Project::load(dir.path())
        .and_then(|p| p.modules())
        .unwrap();
    let client = connect_with(options, Default::default()).await;
    json_of(
        &call(
            &client,
            "scene_new",
            json!({"path": "scenes/a.scene.ron", "name": "A"}),
        )
        .await,
    );
    json_of(
        &call(
            &client,
            "entity_spawn",
            json!({"components": {"Name": "Box", "Velocity": {"linear": [0.0, 1.0, 0.0], "angular": [0.0, 0.0, 0.0]},
                   "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]}}}),
        )
        .await,
    );
    let started = json_of(
        &call(
            &client,
            "replay_record",
            json!({"path": PATH, "checkpoint_every": 30}),
        )
        .await,
    );
    assert_eq!(started["state"], "recording");
    json_of(
        &call(
            &client,
            "input_inject",
            json!({"events": [{"tick": 5, "event": {"Key": {"key": "Space", "pressed": true}}}]}),
        )
        .await,
    );
    let stepped = json_of(&call(&client, "sim_step", json!({"ticks": 90})).await);
    let stopped = json_of(&call(&client, "replay_record", json!({"action": "stop"})).await);
    assert_eq!(
        (stopped["tick"].clone(), stopped["inputs"].clone()),
        (json!(90), json!(2))
    );

    let run = json_of(&call(&client, "replay_run", json!({"path": PATH})).await);
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["matched"], 4);
    assert_eq!(run["world_hash"], stepped["world_hash"]);
    let text = std::fs::read_to_string(dir.path().join(PATH)).unwrap();
    concordia_replay::read_replay(&text).expect("a valid replay");

    // assert_eval over the replay (M3 3.2b): the box rises at 1 m/s, y = t/60, so it is above
    // 0.51 m first after tick 31 (0.5167; 0.5 at 30).
    let checked = json_of(
        &call(
            &client,
            "assert_eval",
            json!({"assertions": ["within 90 ticks: name(\"Box\").Transform.translation.y > 0.51"],
                   "source": "replay", "path": PATH}),
        )
        .await,
    );
    assert_eq!(checked["results"][0]["status"], "passed", "{checked}");
    assert_eq!(checked["results"][0]["decided_at"], 31, "{checked}");
}
