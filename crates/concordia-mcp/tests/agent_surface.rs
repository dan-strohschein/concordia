//! The agent surface (M1 phase 1.12, ADR-0049): with the full M1 module set, every agent method is a
//! described MCP tool, editor plumbing is not, and the agent guide covers every tool.

use crate::common;

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use common::{Client, call, connect_with, options};
use concordia_api::{Audience, Engine, EngineConfig, EngineModule};
use rmcp::model::{ClientConfig, ReadResourceRequestParams, ResourceContents};
use serde_json::Value;

const EDITOR_ONLY: [&str; 5] = [
    "viewport.gizmo",
    "viewport.navigate",
    "viewport.overlays",
    "viewport.present",
    "viewport.stats",
];

/// A temp project with the M1 example's manifest and input map (no scenes, no engine state).
fn m1_project() -> (tempfile::TempDir, Vec<Arc<dyn EngineModule>>) {
    let dir = tempfile::tempdir().unwrap();
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/m1-physics-stack");
    for rel in ["concordia.toml", "input/default.input.ron"] {
        let to = dir.path().join(rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(example.join(rel), to).unwrap();
    }
    let modules = concordia_app::Project::load(dir.path())
        .and_then(|p| p.modules())
        .unwrap();
    (dir, modules)
}

async fn m1_client() -> (tempfile::TempDir, Client, Engine) {
    let (dir, modules) = m1_project();
    let mut config = EngineConfig::new(dir.path());
    config.modules = modules.clone();
    let engine = Engine::new(config).unwrap();
    let mut opts = options(&dir);
    opts.modules = modules;
    let client = connect_with(opts, ClientConfig::default()).await;
    (dir, client, engine)
}

async fn tool_names(client: &Client) -> BTreeSet<String> {
    client
        .list_all_tools()
        .await
        .unwrap()
        .iter()
        .map(|t| t.name.to_string())
        .collect()
}

#[tokio::test]
async fn every_agent_method_is_an_mcp_tool_and_no_editor_method_is() {
    let (_dir, client, engine) = m1_client().await;
    let tools = tool_names(&client).await;
    let expected: BTreeSet<String> = engine
        .methods()
        .iter()
        .filter(|m| m.audience == Audience::agent)
        .filter(|m| m.name != "session.open" && !m.name.starts_with("events."))
        .map(|m| m.name.replace('.', "_"))
        .collect();
    assert_eq!(tools, expected);
    for name in EDITOR_ONLY {
        let method = engine.methods().get(name).expect(name);
        assert_eq!(method.audience, Audience::editor, "{name}");
        assert!(
            !tools.contains(&name.replace('.', "_")),
            "{name} is offered to agents"
        );
    }
    for needed in [
        "asset_import",
        "capture_screenshot",
        "input_inject",
        "physics_raycast",
        "prefab_instantiate",
        "project_files",
        "viewport_drop",
        "viewport_pick",
    ] {
        assert!(tools.contains(needed), "agents need {needed}");
    }
}

#[tokio::test]
async fn every_tool_and_param_has_a_description() {
    let (_dir, client, _engine) = m1_client().await;
    let mut missing = Vec::new();
    for tool in client.list_all_tools().await.unwrap() {
        let docs = tool.description.as_deref().unwrap_or_default();
        let summary = docs
            .split("\n\nExample arguments:")
            .next()
            .unwrap_or_default();
        if summary.trim().len() < 20 {
            missing.push(format!("{}: tool description {summary:?}", tool.name));
        }
        let schema = serde_json::to_value(tool.input_schema.as_ref()).unwrap();
        for (param, spec) in schema["properties"].as_object().into_iter().flatten() {
            let described = spec["description"]
                .as_str()
                .is_some_and(|d| !d.trim().is_empty());
            if !described {
                missing.push(format!("{}.{param}", tool.name));
            }
        }
    }
    assert!(missing.is_empty(), "undescribed:\n{}", missing.join("\n"));
}

#[tokio::test]
async fn agent_guide_mentions_every_agent_tool() {
    let (_dir, client, _engine) = m1_client().await;
    let guide = client
        .read_resource(ReadResourceRequestParams::new(
            "concordia://docs/agent-guide",
        ))
        .await
        .unwrap();
    let text: String = guide
        .contents
        .iter()
        .filter_map(|c| match c {
            ResourceContents::TextResourceContents { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let missing: Vec<String> = tool_names(&client)
        .await
        .into_iter()
        .map(|tool| tool.replacen('_', ".", 1))
        .filter(|method| !text.contains(&format!("`{method}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "the agent guide does not mention: {missing:?}"
    );
}

#[test]
fn capture_without_size_fits_the_agent_budget() {
    let params: concordia_render::CaptureScreenshotParams =
        serde_json::from_value(Value::Object(Default::default())).unwrap();
    assert!(
        params.width.max(params.height) <= 768,
        "default capture is {}x{}",
        params.width,
        params.height
    );
}

#[test]
fn schema_index_has_category_and_description_for_every_component() {
    let (dir, modules) = m1_project();
    let mut config = EngineConfig::new(dir.path());
    config.modules = modules;
    let mut engine = Engine::new(config).unwrap();
    let session = engine.open_session();
    let index = engine
        .call(session, "schema.get", serde_json::json!({}))
        .unwrap()["schema"]
        .clone();
    let components = index["components"].as_array().expect("components list");
    let names: BTreeSet<&str> = components
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    for needed in [
        "Transform",
        "PointLight",
        "MeshRenderer",
        "RigidBody",
        "Collider",
    ] {
        assert!(names.contains(needed), "{needed} missing from {names:?}");
    }
    let mut bad = Vec::new();
    for c in components {
        let text = |key: &str| c[key].as_str().is_some_and(|s| !s.trim().is_empty());
        if !text("category") || !text("description") || !c["addable"].is_boolean() {
            bad.push(c.to_string());
        }
    }
    assert!(
        bad.is_empty(),
        "incomplete component rows:\n{}",
        bad.join("\n")
    );
    let point = components
        .iter()
        .find(|c| c["name"] == "PointLight")
        .unwrap();
    assert_eq!(point["category"], "Lighting");
    assert_eq!(point["addable"], true);
}

/// The PNG data of a capture_screenshot result, or None when there is no GPU (and none is required).
fn image_data(result: &rmcp::model::CallToolResult) -> Option<String> {
    let json = serde_json::to_value(result).unwrap();
    if result.is_error == Some(true) {
        let gpu_missing = json.to_string().contains("GpuUnavailable");
        assert!(
            gpu_missing && std::env::var("CONCORDIA_REQUIRE_GPU").as_deref() != Ok("1"),
            "capture failed: {json}"
        );
        return None;
    }
    let images: Vec<String> = json["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "image")
        .map(|c| c["data"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(images.len(), 1, "{json}");
    images.into_iter().next()
}

#[tokio::test]
async fn capture_screenshot_view_mode_wireframe_differs_from_lit() {
    let (_dir, client, _engine) = m1_client().await;
    call(
        &client,
        "scene_new",
        serde_json::json!({"path": "scenes/v.scene.ron", "name": "V"}),
    )
    .await;
    for template in ["Cube", "Directional Light"] {
        call(
            &client,
            "entity_create",
            serde_json::json!({"template": template}),
        )
        .await;
    }
    call(
        &client,
        "entity_create",
        serde_json::json!({"template": "Camera", "translation": [0.0, 1.0, 4.0]}),
    )
    .await;
    let shot = |mode: &'static str| {
        let client = &client;
        async move {
            let args = serde_json::json!({"width": 160, "height": 90, "view_mode": mode});
            image_data(&call(client, "capture_screenshot", args).await)
        }
    };
    let Some(lit) = shot("lit").await else { return };
    let wire = shot("wireframe")
        .await
        .expect("lit rendered, so wireframe should");
    assert_ne!(lit, wire, "wireframe looks different from lit");
    assert_eq!(
        shot("lit").await.unwrap(),
        lit,
        "and lit is unchanged by it"
    );
}
