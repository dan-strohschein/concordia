//! In-process MCP server and client over a duplex pipe.
#![allow(dead_code, missing_docs)]

use concordia_mcp::{ConcordiaServer, Options};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, ClientConfig};
use rmcp::service::{RoleClient, RunningService};
use serde_json::Value;

pub type Client = RunningService<RoleClient, ClientConfig>;

pub fn options(dir: &tempfile::TempDir) -> Options {
    let mut options = Options::new(dir.path());
    options.deterministic_ids = Some(7);
    options.threads = std::num::NonZeroUsize::new(1);
    options
}

pub async fn connect_with(options: Options, info: ClientConfig) -> Client {
    let server = ConcordiaServer::start(options)
        .await
        .expect("server starts");
    let (server_io, client_io) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_io).await {
            let _ = running.waiting().await;
        }
    });
    info.serve(client_io).await.expect("handshake")
}

pub async fn connect(dir: &tempfile::TempDir) -> Client {
    connect_with(options(dir), ClientConfig::default()).await
}

pub async fn call(client: &Client, tool: &str, arguments: Value) -> CallToolResult {
    let Value::Object(arguments) = arguments else {
        panic!("arguments must be an object")
    };
    client
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments))
        .await
        .unwrap_or_else(|e| panic!("{tool}: protocol error {e}"))
}

pub fn text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}
