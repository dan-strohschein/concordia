//! Terra tools over MCP (M2 phase 2.0c): generated schemas, and an agent spawning from a
//! project template.

use crate::common;

use common::{call, connect, text};
use concordia_api::{methods, params_schema, result_schema};
use serde_json::{Value, json};

const SWARM: &str = r#"Template(
    format: 1,
    description: "Drifting markers for a crowd test.",
    traits: {
        "Movement": (
            velocity: (0.0, 0.0, 2.0),
        ),
    },
    components: {
        "Name": "Mote",
    },
)
"#;

fn json_of(result: &rmcp::model::CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "{}", text(result));
    serde_json::from_str(&text(result)).unwrap()
}

#[test]
fn terra_methods_have_generated_schemas_and_mcp_tools() {
    for name in [
        "terra.spawn_batch",
        "terra.despawn_batch",
        "terra.stats",
        "terra.inspect",
    ] {
        let method = methods()
            .get(name)
            .unwrap_or_else(|| panic!("{name} missing"));
        assert!(concordia_mcp::is_tool(method), "{name} is an MCP tool");
        assert_eq!(params_schema(method)["type"], "object", "{name}");
        assert_eq!(result_schema(method)["type"], "object", "{name}");
        assert!(method.docs.len() > 20, "{name} is described");
    }
}

#[tokio::test]
async fn agent_spawns_from_a_template_over_mcp() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("templates")).unwrap();
    std::fs::write(dir.path().join("templates/mote.template.ron"), SWARM).unwrap();
    let client = connect(&dir).await;
    json_of(
        &call(
            &client,
            "scene_new",
            json!({"path": "scenes/a.scene.ron", "name": "A"}),
        )
        .await,
    );
    let listed = json_of(&call(&client, "entity_templates", json!({})).await);
    assert!(listed.to_string().contains("\"mote\""), "{listed}");
    let spawned = json_of(
        &call(
            &client,
            "terra_spawn_batch",
            json!({"template": "mote", "count": 500, "seed": 9,
                   "distribution": {"Box": {"half_extents": [10.0, 0.0, 10.0]}}}),
        )
        .await,
    );
    assert_eq!(spawned["count"], 500);
    let first = spawned["entities"][0].clone();
    json_of(&call(&client, "sim_step", json!({"ticks": 30})).await);
    let stats = json_of(&call(&client, "terra_stats", json!({"world": "play"})).await);
    assert_eq!(stats["entities"], 500);
    let inspect = json_of(
        &call(
            &client,
            "terra_inspect",
            json!({"entity": first, "world": "play"}),
        )
        .await,
    );
    assert!(inspect.to_string().contains("Velocity"), "{inspect}");
}
