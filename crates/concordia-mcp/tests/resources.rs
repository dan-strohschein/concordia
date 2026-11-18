//! MCP resources: schemas, docs and the current scene (M0 phase 0.7).

mod common;

use common::{call, connect};
use rmcp::model::{ReadResourceRequestParams, ResourceContents};
use serde_json::{Value, json};

async fn read(client: &common::Client, uri: &str) -> Result<String, String> {
    match client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await
    {
        Ok(result) => match result.contents.into_iter().next() {
            Some(ResourceContents::TextResourceContents { text, .. }) => Ok(text),
            other => Err(format!("unexpected {other:?}")),
        },
        Err(e) => Err(e.to_string()),
    }
}

#[tokio::test]
async fn resources_list_includes_schema_index_docs_and_current_scene() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    let uris: Vec<String> = client
        .list_all_resources()
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.uri)
        .collect();
    for uri in [
        "concordia://schema/index",
        "concordia://docs/agent-guide",
        "concordia://docs/errors",
        "concordia://scene/current",
    ] {
        assert!(
            uris.contains(&uri.to_owned()),
            "{uri} missing from {uris:?}"
        );
    }
    let guide = read(&client, "concordia://docs/agent-guide").await.unwrap();
    assert!(guide.contains("## Workflow"));
    let errors = read(&client, "concordia://docs/errors").await.unwrap();
    assert!(
        errors.contains("-32005") && errors.contains("TxnConflict"),
        "{errors}"
    );
    let index: Value =
        serde_json::from_str(&read(&client, "concordia://schema/index").await.unwrap()).unwrap();
    assert!(
        index["types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "Transform")
    );
}

#[tokio::test]
async fn resource_templates_resolve_component_and_method_schemas() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    let templates: Vec<String> = client
        .list_resource_templates(None)
        .await
        .unwrap()
        .resource_templates
        .into_iter()
        .map(|t| t.uri_template)
        .collect();
    assert_eq!(
        templates,
        [
            "concordia://schema/component/{name}",
            "concordia://schema/method/{name}"
        ]
    );
    let transform: Value = serde_json::from_str(
        &read(&client, "concordia://schema/component/Transform")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        transform["description"],
        "Local transform relative to `Parent` (or to the world if there is no parent)."
    );
    let method: Value = serde_json::from_str(
        &read(&client, "concordia://schema/method/entity.spawn")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(method["mutates"], true);
}

#[tokio::test]
async fn scene_current_resource_is_canonical_ron() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    assert!(
        read(&client, "concordia://scene/current").await.is_err(),
        "no scene yet"
    );
    call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await;
    call(
        &client,
        "entity_spawn",
        json!({"components": {"Name": "Lamp"}}),
    )
    .await;
    let current = read(&client, "concordia://scene/current").await.unwrap();
    call(&client, "scene_save", json!({})).await;
    assert_eq!(
        current,
        std::fs::read_to_string(dir.path().join("scenes/a.scene.ron")).unwrap()
    );
}

#[tokio::test]
async fn unknown_resources_are_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    for uri in [
        "concordia://nope",
        "concordia://schema/component/Transfrom",
        "concordia://schema/method/entity.spwan",
        "https://example.com",
    ] {
        assert!(read(&client, uri).await.is_err(), "{uri}");
    }
}
