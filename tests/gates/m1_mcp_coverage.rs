//! M1 gate (ADR-0049): with the full M1 module set, the real `concordia-mcp` binary offers every
//! agent-audience method as a described tool, and no editor-audience method.

use std::collections::BTreeSet;

use concordia_api::{Audience, Engine, EngineConfig};
use concordia_tests::bins::concordia_mcp_bin;
use concordia_tests::mcp_client::McpClient;

#[tokio::test]
async fn m1_gate_every_agent_method_is_a_described_mcp_tool() {
    let dir = tempfile::tempdir().unwrap();
    concordia_tests::copy_example("m1-physics-stack", dir.path()).unwrap();
    let mut config = EngineConfig::new(dir.path());
    config.modules = concordia_app::Project::load(dir.path())
        .and_then(|p| p.modules())
        .unwrap();
    let engine = Engine::new(config).unwrap();
    let project = dir.path().to_str().unwrap();
    let client = McpClient::spawn(&concordia_mcp_bin(), &["--project", project])
        .await
        .unwrap();

    let tools = client.tools().await;
    let names: BTreeSet<String> = tools.iter().map(|t| t.name.to_string()).collect();
    let agent: BTreeSet<String> = engine
        .methods()
        .iter()
        .filter(|m| m.audience == Audience::agent)
        .filter(|m| m.name != "session.open" && !m.name.starts_with("events."))
        .map(|m| m.name.replace('.', "_"))
        .collect();
    assert_eq!(names, agent, "the tools are exactly the agent methods");
    for method in engine
        .methods()
        .iter()
        .filter(|m| m.audience == Audience::editor)
    {
        assert!(
            !names.contains(&method.name.replace('.', "_")),
            "{} is editor-only",
            method.name
        );
    }
    let mut undescribed = Vec::new();
    for tool in &tools {
        let docs = tool.description.as_deref().unwrap_or_default();
        if docs.trim().len() < 20 {
            undescribed.push(tool.name.to_string());
        }
        let schema = serde_json::to_value(tool.input_schema.as_ref()).unwrap();
        for (param, spec) in schema["properties"].as_object().into_iter().flatten() {
            if spec["description"]
                .as_str()
                .is_none_or(|d| d.trim().is_empty())
            {
                undescribed.push(format!("{}.{param}", tool.name));
            }
        }
    }
    assert!(undescribed.is_empty(), "undescribed: {undescribed:?}");
    client.shutdown().await;
}
