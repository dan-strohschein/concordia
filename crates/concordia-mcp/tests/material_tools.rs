//! Material tools over MCP (M2 phase 2.3c): an agent reads a material, changes a slab and
//! writes it back; an invalid one is refused naming the node and field.

use crate::common;

use common::{call, connect_with, options, text};
use serde_json::{Value, json};

const PATH: &str = "assets/materials/steel.material.ron";

fn json_of(result: &rmcp::model::CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "{}", text(result));
    serde_json::from_str(&text(result)).unwrap()
}

#[tokio::test]
async fn agent_edits_a_slab_over_mcp() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), "[assets]\n").unwrap();
    std::fs::create_dir_all(dir.path().join("assets/materials")).unwrap();
    std::fs::write(
        dir.path().join(PATH),
        "(base_color: (0.6, 0.6, 0.62, 1.0), metallic: 1.0, roughness: 0.4)\n",
    )
    .unwrap();
    let mut options = options(&dir);
    options.modules = concordia_app::Project::load(dir.path())
        .and_then(|p| p.modules())
        .unwrap();
    let client = connect_with(options, Default::default()).await;
    let tools = client.list_all_tools().await.unwrap();
    for name in ["material_get", "material_set"] {
        assert!(
            tools.iter().any(|t| t.name == name),
            "{name} is offered with [assets]"
        );
    }

    let got = json_of(&call(&client, "material_get", json!({"path": PATH})).await);
    assert_eq!(got["migrated"], true, "{got}");
    let mut material = got["material"].clone();
    material["nodes"]["surface"]["Slab"]["roughness"] = json!(0.2);
    let set = json_of(
        &call(
            &client,
            "material_set",
            json!({"path": PATH, "material": material}),
        )
        .await,
    );
    assert!(set["id"].is_string(), "{set}");
    let again = json_of(&call(&client, "material_get", json!({"path": PATH})).await);
    let roughness = again["material"]["nodes"]["surface"]["Slab"]["roughness"]
        .as_f64()
        .unwrap();
    assert!((roughness - 0.2).abs() < 1e-6, "{roughness}");
    assert_eq!(again["migrated"], false);

    material["nodes"]["surface"]["Slab"]["base_color"] = json!([2.0, 0.0, 0.0, 1.0]);
    let refused = call(
        &client,
        "material_set",
        json!({"path": PATH, "material": material}),
    )
    .await;
    assert_eq!(refused.is_error, Some(true));
    assert!(
        text(&refused).contains("surface.base_color[0]"),
        "{}",
        text(&refused)
    );
}
