//! Tools: 1:1 with Engine API methods, generated schemas, tool errors (M0 phase 0.7).

mod common;

use common::{call, connect, connect_with, options, text};
use concordia_api::{methods, params_schema, result_schema};
use concordia_mcp::{ConcordiaServer, StartError};
use rmcp::model::{ClientConfig, ProtocolVersion};
use serde_json::{Value, json};

fn tool_name(method: &str) -> String {
    method.replace('.', "_")
}

#[tokio::test]
async fn tool_list_is_one_to_one_with_api_methods() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    let names: Vec<String> = client
        .list_all_tools()
        .await
        .unwrap()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    let expected: Vec<String> = methods()
        .names()
        .filter(|m| concordia_mcp::is_tool(m))
        .map(tool_name)
        .collect();
    assert_eq!(names, expected);
}

#[tokio::test]
async fn tool_input_schemas_equal_generated_param_schemas() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    for tool in client.list_all_tools().await.unwrap() {
        let method = methods().get(&tool.name.replace('_', ".")).unwrap();
        assert_eq!(
            serde_json::to_string(tool.input_schema.as_ref()).unwrap(),
            serde_json::to_string(&params_schema(method)).unwrap(),
            "{}",
            tool.name
        );
    }
}

#[tokio::test]
async fn tool_output_schemas_present() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    for tool in client.list_all_tools().await.unwrap() {
        let method = methods().get(&tool.name.replace('_', ".")).unwrap();
        let output = tool
            .output_schema
            .as_ref()
            .unwrap_or_else(|| panic!("{} has no output schema", tool.name));
        assert_eq!(
            Value::Object(output.as_ref().clone()),
            result_schema(method),
            "{}",
            tool.name
        );
    }
}

#[tokio::test]
async fn tool_annotations_reflect_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    for tool in client.list_all_tools().await.unwrap() {
        let method = methods().get(&tool.name.replace('_', ".")).unwrap();
        let annotations = tool.annotations.as_ref().unwrap();
        assert_eq!(
            annotations.read_only_hint,
            Some(!method.mutates),
            "{}",
            tool.name
        );
        let destructive =
            ["entity_despawn", "component_remove", "scene_new"].contains(&tool.name.as_ref());
        if method.mutates {
            assert_eq!(
                annotations.destructive_hint,
                Some(destructive),
                "{}",
                tool.name
            );
        }
        let description = tool.description.as_deref().unwrap_or("");
        assert!(
            description.starts_with(method.docs.lines().next().unwrap()),
            "{}",
            tool.name
        );
        assert!(description.contains("Example arguments:"), "{}", tool.name);
    }
}

#[tokio::test]
async fn tool_list_token_budget() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    let tools = client.list_all_tools().await.unwrap();
    let chars = serde_json::to_string(&tools).unwrap().len();
    assert!(
        chars / 4 < 12_000,
        "tools/list is about {} tokens",
        chars / 4
    );
}

#[tokio::test]
async fn tool_call_success_returns_structured_and_text_content() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    let result = call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await;
    assert_ne!(result.is_error, Some(true));
    let structured = result.structured_content.clone().unwrap();
    assert_eq!(structured["path"], "scenes/a.scene.ron");
    assert_eq!(
        serde_json::from_str::<Value>(&text(&result)).unwrap(),
        structured
    );
}

#[tokio::test]
async fn tool_call_engine_error_is_tool_error_not_protocol_error() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await;
    let spawned = call(
        &client,
        "entity_spawn",
        json!({"components": {"Name": "A"}}),
    )
    .await;
    let id = spawned.structured_content.unwrap()["id"].clone();
    let result = call(
        &client,
        "component_set",
        json!({"entity": id, "component": "Transfrom", "value": {}}),
    )
    .await;
    assert_eq!(result.is_error, Some(true));
    let message = text(&result);
    for needle in [
        "Transfrom",
        "did you mean \"Transform\"?",
        "concordia://schema/index",
    ] {
        assert!(
            message.contains(needle),
            "{needle:?} missing from {message}"
        );
    }
    assert_eq!(result.structured_content.unwrap()["code"], -32002);
}

#[tokio::test]
async fn unknown_tool_is_a_protocol_error() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    let params = rmcp::model::CallToolRequestParams::new("entity_spwan");
    let err = client.call_tool(params).await.unwrap_err();
    assert!(err.to_string().contains("entity_spawn"), "{err}");
}

#[tokio::test]
async fn default_actor_from_flag_is_applied() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(&dir);
    opts.actor = "agent:builder#3".parse().unwrap();
    let client = connect_with(opts, ClientConfig::default()).await;
    call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await;
    call(&client, "entity_spawn", json!({"components": {}})).await;
    let history = call(&client, "txn_history", json!({}))
        .await
        .structured_content
        .unwrap();
    assert_eq!(history["entries"][0]["actor"], "agent:builder#3");
}

#[tokio::test]
async fn human_actor_rejected_over_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(&dir);
    opts.actor = "human:local".parse().unwrap();
    assert!(matches!(
        ConcordiaServer::start(opts).await,
        Err(StartError::HumanActor(_))
    ));

    let client = connect(&dir).await;
    call(
        &client,
        "scene_new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    )
    .await;
    let result = call(
        &client,
        "entity_spawn",
        json!({"components": {}, "actor": "human:local"}),
    )
    .await;
    assert_eq!(result.is_error, Some(true));
    assert!(text(&result).contains("human"), "{}", text(&result));
}

#[tokio::test]
async fn handshake_works_with_protocol_2025_11_25_and_2026_07_28() {
    for version in [ProtocolVersion::V_2025_11_25, ProtocolVersion::V_2026_07_28] {
        let dir = tempfile::tempdir().unwrap();
        let mut info = ClientConfig::default();
        info.protocol_version = version.clone();
        let client = connect_with(options(&dir), info).await;
        let server = client
            .peer_info()
            .and_then(|i| i.server_info.clone())
            .map(|s| s.name);
        assert_eq!(server.as_deref(), Some("concordia"), "{version:?}");
        assert_eq!(
            client.list_all_tools().await.unwrap().len(),
            24,
            "{version:?}"
        );
    }
}

#[tokio::test]
async fn unadvertised_methods_not_reachable() {
    let dir = tempfile::tempdir().unwrap();
    let client = connect(&dir).await;
    assert!(
        client.list_prompts(None).await.is_err(),
        "prompts are not advertised"
    );
}

mod module_tools {
    use std::sync::Arc;

    use concordia_api::{
        ApiError, CallCtx, Engine, EngineModule, Method, MethodInfo, params_schema,
    };
    use concordia_reflect::Reflect;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use super::common::{call, connect_with, options, text};

    /// Answer "pong".
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
    struct Ping;

    /// `probe.ping` params.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
    #[serde(deny_unknown_fields)]
    struct PingParams {}

    /// `probe.ping` result.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
    struct Pong {
        /// Always "pong".
        reply: String,
    }

    impl Method for Ping {
        const NAME: &'static str = "probe.ping";
        const MUTATES: bool = false;
        type Params = PingParams;
        type Result = Pong;

        fn call(_: &mut Engine, _: &CallCtx, _: PingParams) -> Result<Pong, ApiError> {
            Ok(Pong {
                reply: "pong".into(),
            })
        }
    }

    #[derive(Debug)]
    struct Probe;

    impl EngineModule for Probe {
        fn name(&self) -> &'static str {
            "probe"
        }

        fn methods(&self) -> Vec<MethodInfo> {
            vec![MethodInfo::of::<Ping>()]
        }
    }

    #[tokio::test]
    async fn tool_list_includes_module_methods() {
        let dir = tempfile::tempdir().unwrap();
        let mut opts = options(&dir);
        opts.modules = vec![Arc::new(Probe)];
        let client = connect_with(opts, Default::default()).await;
        let tools = client.list_all_tools().await.unwrap();
        let ping = tools
            .iter()
            .find(|t| t.name == "probe_ping")
            .expect("probe_ping is listed");
        assert_eq!(
            serde_json::to_value(ping.input_schema.as_ref()).unwrap(),
            params_schema(&MethodInfo::of::<Ping>())
        );
        let result = call(&client, "probe_ping", json!({})).await;
        assert_ne!(result.is_error, Some(true), "{}", text(&result));
        assert!(text(&result).contains("pong"), "{}", text(&result));
        let schema = client
            .read_resource(rmcp::model::ReadResourceRequestParams::new(
                "concordia://schema/method/probe.ping",
            ))
            .await
            .expect("module method schema resource");
        assert!(format!("{schema:?}").contains("probe.ping"));
    }

    /// Return a tiny image.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
    struct Snap;

    /// `probe.snap` result: an image plus metadata.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
    struct SnapResult {
        /// The picture.
        image: concordia_api::EncodedImage,
        /// Width.
        width: u32,
    }

    impl Method for Snap {
        const NAME: &'static str = "probe.snap";
        const MUTATES: bool = false;
        type Params = PingParams;
        type Result = SnapResult;

        fn call(_: &mut Engine, _: &CallCtx, _: PingParams) -> Result<SnapResult, ApiError> {
            Ok(SnapResult {
                image: concordia_api::EncodedImage::png(b"not really a png"),
                width: 3,
            })
        }
    }

    #[derive(Debug)]
    struct Camera;

    impl EngineModule for Camera {
        fn name(&self) -> &'static str {
            "camera"
        }

        fn methods(&self) -> Vec<MethodInfo> {
            vec![MethodInfo::of::<Snap>()]
        }
    }

    #[tokio::test]
    async fn image_results_become_mcp_image_content() {
        use base64::Engine as _;
        let dir = tempfile::tempdir().unwrap();
        let mut opts = options(&dir);
        opts.modules = vec![Arc::new(Camera)];
        let client = connect_with(opts, Default::default()).await;
        let result = call(&client, "probe_snap", json!({})).await;
        assert_ne!(result.is_error, Some(true), "{}", text(&result));
        let image = result.content[0]
            .as_image()
            .expect("first item is an image");
        assert_eq!(image.mime_type, "image/png");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&image.data)
                .unwrap(),
            b"not really a png"
        );
        let rest: serde_json::Value = serde_json::from_str(&text(&result)).unwrap();
        assert_eq!(
            rest,
            json!({"width": 3}),
            "metadata is a text item without the image"
        );
        assert!(
            result.structured_content.is_none(),
            "no duplicate base64 in structured content"
        );
        let tools = client.list_all_tools().await.unwrap();
        let snap = tools.iter().find(|t| t.name == "probe_snap").unwrap();
        assert!(
            snap.output_schema.is_none(),
            "image tools return content, not structured output"
        );
    }
}
