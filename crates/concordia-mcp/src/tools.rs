//! rmcp adapter, part 1: the server handler and tools (one per Engine API method).

use std::sync::{Arc, OnceLock};

use concordia_api::{ApiError, ApiErrorKind, MethodInfo, methods, params_schema, result_schema};
use concordia_core::suggest::did_you_mean;
use concordia_reflect::example_json;
use rmcp::ServiceExt;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
    PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse, ServerCapabilities,
    ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::Value;

use crate::{ConcordiaServer, resources};

const INSTRUCTIONS: &str = "Concordia game engine, headless. Start with scene_new or scene_open, \
discover component shapes with schema_get, edit with entity_spawn / component_set / component_patch, \
group edits in txn_begin..txn_commit, undo with txn_undo, save with scene_save. \
Read concordia://docs/agent-guide for the full workflow and error codes.";

/// Mutating tools that delete or replace data.
const DESTRUCTIVE: [&str; 3] = ["entity_despawn", "component_remove", "scene_new"];

/// MCP tool name for an API method (`entity.spawn` → `entity_spawn`).
fn tool_name(method: &str) -> String {
    method.replace('.', "_")
}

/// The API methods exposed as tools (`session.open` is handled by the adapter).
fn tool_methods() -> impl Iterator<Item = &'static MethodInfo> {
    methods().iter().filter(|m| m.name != "session.open")
}

fn tools() -> &'static [Tool] {
    static TOOLS: OnceLock<Vec<Tool>> = OnceLock::new();
    TOOLS.get_or_init(|| tool_methods().map(tool).collect())
}

fn tool(method: &MethodInfo) -> Tool {
    let name = tool_name(method.name);
    let annotations = ToolAnnotations::new()
        .read_only(!method.mutates)
        .destructive(method.mutates && DESTRUCTIVE.contains(&name.as_str()));
    Tool::new(
        name,
        description(method),
        Arc::new(object(params_schema(method))),
    )
    .with_raw_output_schema(Arc::new(object(result_schema(method))))
    .with_annotations(annotations)
}

/// Doc comment plus one example call (null fields omitted).
fn description(method: &MethodInfo) -> String {
    let mut example = example_json(method.params);
    if let Value::Object(fields) = &mut example {
        fields.retain(|_, v| !v.is_null());
    }
    format!("{}\n\nExample arguments: {example}", method.docs)
}

fn object(schema: Value) -> JsonObject {
    match schema {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    }
}

/// Text an agent reads to self-correct: message, suggestions, resource to read, raw data.
fn error_text(error: &ApiError) -> String {
    let mut text = error.message.clone();
    if !error.data.suggestions.is_empty() {
        text.push_str("\n\nSuggestions:");
        for suggestion in &error.data.suggestions {
            text.push_str(&format!("\n- {suggestion}"));
        }
    }
    if let Some(see) = &error.data.see {
        text.push_str(&format!("\n\nSee: {see}"));
    }
    text.push_str(&format!("\n\nError: {}", error.to_jsonrpc()));
    text
}

fn tool_error(error: &ApiError) -> CallToolResult {
    let mut result = CallToolResult::error(vec![ContentBlock::text(error_text(error))]);
    result.structured_content = Some(error.to_jsonrpc());
    result
}

/// `human:*` actors may not act over MCP.
fn reject_human(arguments: &JsonObject) -> Option<ApiError> {
    let actor = arguments.get("actor")?.as_str()?;
    actor.starts_with("human:").then(|| {
        ApiError::new(
            ApiErrorKind::ActorMalformed,
            format!("{actor} is a human actor; MCP calls act as agents"),
        )
        .suggest([
            "omit \"actor\" to use the server's agent actor, or pass agent:<role>".to_owned(),
        ])
        .at("/actor")
    })
}

impl ConcordiaServer {
    async fn invoke(&self, request: CallToolRequestParams) -> Result<CallToolResult, McpError> {
        let name = request.name.as_ref();
        let Some(method) = tool_methods().find(|m| tool_name(m.name) == name) else {
            let names: Vec<String> = tool_methods().map(|m| tool_name(m.name)).collect();
            let similar = did_you_mean(name, names.iter().map(String::as_str));
            let hint = similar
                .first()
                .map(|s| format!("; did you mean {s}?"))
                .unwrap_or_default();
            return Err(McpError::invalid_params(
                format!("no tool named {name:?}{hint}"),
                None,
            ));
        };
        let arguments = request.arguments.unwrap_or_default();
        if let Some(error) = reject_human(&arguments) {
            return Ok(tool_error(&error));
        }
        Ok(
            match self
                .handle
                .call(self.session, method.name, Value::Object(arguments))
                .await
            {
                Ok(value) => CallToolResult::structured(value),
                Err(error) => tool_error(&error),
            },
        )
    }

    pub(crate) async fn engine_call(&self, method: &str, params: Value) -> Result<Value, ApiError> {
        self.handle.call(self.session, method, params).await
    }

    pub(crate) async fn scene_text(&self) -> Result<String, ApiError> {
        self.handle.scene_text().await
    }
}

impl ServerHandler for ConcordiaServer {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        ServerConfig::new(capabilities)
            .with_server_info(Implementation::new("concordia", concordia_core::version()))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(tools().to_vec()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.invoke(request).await.map(Into::into)
    }

    async fn list_resources(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(resources::list())
    }

    async fn list_resource_templates(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(resources::templates())
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        resources::read(self, &request.uri).await.map(Into::into)
    }
}

/// Serves MCP on stdin/stdout until the client disconnects.
pub(crate) async fn serve_stdio(
    server: ConcordiaServer,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}
