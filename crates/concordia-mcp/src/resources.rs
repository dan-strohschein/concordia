//! rmcp adapter, part 2: `concordia://` resources (schemas, docs, the current scene).

use concordia_api::{ApiError, ApiErrorKind};
use rmcp::ErrorData as McpError;
use rmcp::model::{
    ListResourceTemplatesResult, ListResourcesResult, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate,
};
use serde_json::{Value, json};

use crate::ConcordiaServer;

const AGENT_GUIDE: &str = include_str!("../../../docs/guides/agents-engine-api.md");
const COMPONENT: &str = "concordia://schema/component/";
const METHOD: &str = "concordia://schema/method/";

pub(crate) fn list() -> ListResourcesResult {
    let resource = |uri: &str, name: &str, description: &str, mime: &str| {
        let mut resource = Resource::new(uri, name).with_description(description);
        resource.mime_type = Some(mime.to_owned());
        resource
    };
    ListResourcesResult::with_all_items(vec![
        resource(
            "concordia://schema/index",
            "schema-index",
            "Every registered type and method.",
            "application/json",
        ),
        resource(
            "concordia://docs/agent-guide",
            "agent-guide",
            "How to drive the engine: workflow, methods, errors.",
            "text/markdown",
        ),
        resource(
            "concordia://docs/errors",
            "errors",
            "Error codes, kinds and what to do.",
            "text/markdown",
        ),
        resource(
            "concordia://scene/current",
            "current-scene",
            "The open scene as canonical RON (unsaved changes included).",
            "text/plain",
        ),
    ])
}

pub(crate) fn templates() -> ListResourceTemplatesResult {
    let template = |uri: &str, name: &str, description: &str| {
        let mut template = ResourceTemplate::new(uri, name).with_description(description);
        template.mime_type = Some("application/json".to_owned());
        template
    };
    ListResourceTemplatesResult::with_all_items(vec![
        template(
            "concordia://schema/component/{name}",
            "component-schema",
            "JSON Schema of a component or type, e.g. Transform.",
        ),
        template(
            "concordia://schema/method/{name}",
            "method-schema",
            "Params and result schemas of a method, e.g. entity.spawn.",
        ),
    ])
}

pub(crate) async fn read(
    server: &ConcordiaServer,
    uri: &str,
) -> Result<ReadResourceResult, McpError> {
    let text = match uri {
        "concordia://schema/index" => json_text(server.engine_call("schema.get", json!({})).await)?,
        "concordia://docs/agent-guide" => AGENT_GUIDE.to_owned(),
        "concordia://docs/errors" => errors_markdown(),
        "concordia://scene/current" => server.scene_text().await.map_err(not_found)?,
        _ => match (uri.strip_prefix(COMPONENT), uri.strip_prefix(METHOD)) {
            (Some(name), _) if server.methods().get(name).is_none() => json_text(
                server
                    .engine_call("schema.get", json!({ "name": name }))
                    .await,
            )?,
            (_, Some(name)) if server.methods().get(name).is_some() => json_text(
                server
                    .engine_call("schema.get", json!({ "name": name }))
                    .await,
            )?,
            _ => {
                return Err(McpError::resource_not_found(
                    format!("no resource {uri}; see resources/list"),
                    None,
                ));
            }
        },
    };
    Ok(ReadResourceResult::new(vec![ResourceContents::text(
        text, uri,
    )]))
}

fn json_text(result: Result<Value, ApiError>) -> Result<String, McpError> {
    let schema = result.map_err(not_found)?["schema"].clone();
    serde_json::to_string_pretty(&schema).map_err(|e| McpError::internal_error(e.to_string(), None))
}

fn not_found(error: ApiError) -> McpError {
    let hint = error
        .data
        .suggestions
        .first()
        .map(|s| format!(" ({s})"))
        .unwrap_or_default();
    McpError::resource_not_found(format!("{}{hint}", error.message), None)
}

/// The error table, generated from `ApiErrorKind::ALL`.
fn errors_markdown() -> String {
    let mut text = String::from(
        "# Concordia Engine API errors\n\n| Code | Kind | Meaning | What to do |\n|---|---|---|---|\n",
    );
    for kind in ApiErrorKind::ALL {
        let example = ApiError::from_kind(kind);
        let suggestion = example
            .data
            .suggestions
            .first()
            .cloned()
            .unwrap_or_default();
        text.push_str(&format!(
            "| {} | `{}` | {} | {} |\n",
            kind.code(),
            kind.name(),
            example.message,
            suggestion
        ));
    }
    text
}
