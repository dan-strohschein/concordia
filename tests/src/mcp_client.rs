//! A minimal MCP client over a child process with a cleared environment (no API keys).

use std::path::Path;
use std::process::Stdio;

use rmcp::ServiceExt;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientConfig, ReadResourceRequestParams,
    ResourceContents, Tool,
};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use serde_json::Value;

/// A tool call that returned `isError: true`.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolError {
    /// The text content (message, suggestions, see).
    pub text: String,
    /// Structured error data, if any.
    pub data: Option<Value>,
}

/// A running MCP server child process plus a connected client.
pub struct McpClient {
    service: RunningService<RoleClient, ClientConfig>,
}

impl McpClient {
    /// Spawns `bin args...` with only `PATH`, `HOME`, `SLANG_DIR` and `CONCORDIA_*` in its
    /// environment (no API keys reach the server).
    pub async fn spawn(bin: &Path, args: &[&str]) -> Result<Self, String> {
        Self::spawn_with(bin, args, ClientConfig::default()).await
    }

    /// Like [`McpClient::spawn`] with custom client info (e.g. an older protocol version).
    pub async fn spawn_with(bin: &Path, args: &[&str], info: ClientConfig) -> Result<Self, String> {
        let mut command = tokio::process::Command::new(bin);
        command.args(args).env_clear();
        for (key, value) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if matches!(name.as_ref(), "PATH" | "HOME" | "SLANG_DIR")
                || name.starts_with("CONCORDIA_")
            {
                command.env(key, value);
            }
        }
        let (transport, _stderr) = TokioChildProcess::builder(command)
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("spawn {}: {e}", bin.display()))?;
        let service = info
            .serve(transport)
            .await
            .map_err(|e| format!("handshake: {e}"))?;
        Ok(Self { service })
    }

    /// Every tool, in server order.
    pub async fn tools(&self) -> Vec<Tool> {
        self.service.list_all_tools().await.expect("tools/list")
    }

    /// Calls a tool; `Ok` carries the structured result.
    pub async fn call(&self, tool: &str, arguments: Value) -> Result<Value, ToolError> {
        let result = self.call_raw(tool, arguments).await;
        let text = result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n");
        match result.is_error {
            Some(true) => Err(ToolError {
                text,
                data: result.structured_content,
            }),
            _ => Ok(result.structured_content.unwrap_or(Value::Null)),
        }
    }

    /// Calls a tool and returns the whole MCP result (content items such as images included).
    pub async fn call_raw(&self, tool: &str, arguments: Value) -> CallToolResult {
        let Value::Object(arguments) = arguments else {
            panic!("tool arguments must be an object")
        };
        let params = CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments);
        self.service
            .call_tool(params)
            .await
            .unwrap_or_else(|e| panic!("{tool}: protocol error {e}"))
    }

    /// Reads a text resource.
    pub async fn read(&self, uri: &str) -> String {
        let result = self
            .service
            .read_resource(ReadResourceRequestParams::new(uri))
            .await
            .expect("resources/read");
        match result.contents.into_iter().next() {
            Some(ResourceContents::TextResourceContents { text, .. }) => text,
            other => panic!("{uri}: expected text contents, got {other:?}"),
        }
    }

    /// The server's advertised name.
    pub fn server_name(&self) -> Option<String> {
        self.service
            .peer_info()
            .and_then(|info| info.server_info.as_ref().map(|s| s.name.clone()))
    }

    /// Stops the client and the child process.
    pub async fn shutdown(self) {
        let _ = self.service.cancel().await;
    }
}
