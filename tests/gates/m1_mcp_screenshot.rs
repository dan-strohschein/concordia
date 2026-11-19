//! M1 gate: an agent with no API key opens the physics scene, steps it and receives a screenshot
//! over MCP from the real `concordia-mcp` binary, matching the in-process golden.

use base64::Engine as _;
use concordia_testkit::golden::{RgbaImage, Tolerance, assert_golden};
use concordia_testkit::gpu_required;
use concordia_tests::bins::concordia_mcp_bin;
use concordia_tests::mcp_client::McpClient;
use serde_json::{Value, json};

async fn server(project: &std::path::Path) -> McpClient {
    let project = project.to_str().unwrap();
    McpClient::spawn(
        &concordia_mcp_bin(),
        &["--project", project, "--actor", "agent:gate"],
    )
    .await
    .expect("server starts")
}

/// The id of the entity named `name`.
async fn id_of(client: &McpClient, name: &str) -> String {
    let rows = client
        .call("query_run", json!({"with": ["Name"], "select": ["Name"]}))
        .await
        .unwrap();
    rows["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["components"]["Name"] == name)
        .unwrap_or_else(|| panic!("no entity named {name:?}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn m1_gate_agent_captures_screenshot_over_mcp() {
    let dir = tempfile::tempdir().unwrap();
    concordia_tests::copy_example("m1-physics-stack", dir.path()).unwrap();
    let client = server(dir.path()).await;

    let tools = client.tools().await;
    for needed in [
        "capture_screenshot",
        "scene_open",
        "sim_step",
        "input_inject",
    ] {
        assert!(
            tools.iter().any(|t| t.name == needed),
            "{needed} is offered"
        );
    }
    let capture = tools
        .iter()
        .find(|t| t.name == "capture_screenshot")
        .unwrap();
    insta::assert_json_snapshot!(
        "capture_screenshot_input_schema",
        capture.input_schema.as_ref()
    );

    client
        .call("scene_open", json!({"path": "scenes/stack.scene.ron"}))
        .await
        .unwrap();
    client
        .call("sim_step", json!({"ticks": 180, "actor": "agent:gate"}))
        .await
        .unwrap();
    let camera = id_of(&client, "gate_cam").await;
    let args = json!({"camera": camera, "width": 1280, "height": 720, "world": "play"});
    let result = serde_json::to_value(client.call_raw("capture_screenshot", args).await).unwrap();
    if result["isError"] == true {
        let gpu_missing = result.to_string().contains("GpuUnavailable");
        assert!(gpu_missing && !gpu_required(), "capture failed: {result}");
        return;
    }
    let images: Vec<&Value> = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "image")
        .collect();
    assert_eq!(images.len(), 1, "{result}");
    assert_eq!(images[0]["mimeType"], "image/png");
    let png = base64::engine::general_purpose::STANDARD
        .decode(images[0]["data"].as_str().unwrap())
        .unwrap();
    let image = RgbaImage::from_png(&png).expect("a valid PNG");
    assert_eq!((image.width(), image.height()), (1280, 720));
    let tolerance = Tolerance {
        mean: 0.015,
        p99: 0.25,
    };
    assert_golden("gates", "m1_physics_stack", &image, tolerance);

    let log = client
        .call("log_tail", json!({"limit": 1000}))
        .await
        .unwrap();
    let stepped = log["records"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["message"] == "sim stepped" && r["fields"]["actor"] == "agent:gate");
    assert!(stepped, "log_tail attributes the step to agent:gate");
    client.shutdown().await;
}
