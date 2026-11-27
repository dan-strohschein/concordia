//! Navigation tools over MCP (M2 phase 2.5a): `nav_build` and `nav_path` are served with generated
//! schemas when `[nav]` is on, and an agent builds a navmesh and asks for a path.

use crate::common;

use common::{call, connect_with, options, text};
use concordia_api::{MethodInfo, params_schema, result_schema};
use rmcp::model::CallToolResult;
use serde_json::{Value, json};

fn json_of(result: &CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "{}", text(result));
    serde_json::from_str(&text(result)).unwrap()
}

async fn project() -> (tempfile::TempDir, common::Client) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), "[physics]\n\n[nav]\n").unwrap();
    let mut options = options(&dir);
    options.modules = concordia_app::Project::load(dir.path())
        .and_then(|p| p.modules())
        .unwrap();
    let client = connect_with(options, Default::default()).await;
    (dir, client)
}

#[tokio::test]
async fn nav_tools_have_generated_schemas_and_mcp_tools() {
    let (_dir, client) = project().await;
    let tools = client.list_all_tools().await.unwrap();
    for (name, method) in [
        ("nav_build", MethodInfo::of::<concordia_nav::NavBuild>()),
        ("nav_path", MethodInfo::of::<concordia_nav::NavPathMethod>()),
    ] {
        let tool = tools
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name} is offered with [nav]"));
        assert_eq!(
            serde_json::to_value(tool.input_schema.as_ref()).unwrap(),
            params_schema(&method),
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(tool.output_schema.as_ref().expect("an output schema")).unwrap(),
            result_schema(&method),
            "{name}"
        );
        assert_eq!(
            tool.annotations.as_ref().and_then(|a| a.read_only_hint),
            Some(true),
            "{name}"
        );
        assert!(method.docs.len() > 40, "{name} is described");
    }
}

#[tokio::test]
async fn agent_builds_a_navmesh_and_finds_a_path_over_mcp() {
    let (_dir, client) = project().await;
    let ok = |result: CallToolResult| json_of(&result);
    ok(call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await);
    let pose = |x: f32, y: f32| json!({"translation": [x, y, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]});
    ok(call(
        &client,
        "entity_spawn",
        json!({"components": {
            "Transform": pose(0.0, -0.5),
            "RigidBody": {"kind": "Static"},
            "Collider": {"shape": {"Box": {"half_extents": [10.1, 0.5, 10.1]}}},
        }}),
    )
    .await);
    let none = call(&client, "nav_build", json!({})).await;
    assert_eq!(none.is_error, Some(true));
    assert!(text(&none).contains("NavMeshVolume"), "{}", text(&none));
    ok(call(
        &client,
        "entity_spawn",
        json!({"components": {
            "Transform": pose(0.0, 0.0),
            "NavMeshVolume": {"half_extents": [12.0, 3.0, 12.0]},
        }}),
    )
    .await);
    let built = ok(call(&client, "nav_build", json!({})).await);
    assert_eq!(built["navmeshes"][0]["islands"], 1, "{built}");
    // Open floor, within one tile: a straight path 8 m long.
    let path = ok(call(
        &client,
        "nav_path",
        json!({"from": [-4.0, 0.0, -3.0], "to": [4.0, 0.0, -3.0]}),
    )
    .await);
    assert_eq!(path["waypoints"].as_array().unwrap().len(), 2, "{path}");
    assert!(
        (path["length"].as_f64().unwrap() - 8.0).abs() < 1e-3,
        "{path}"
    );
}
