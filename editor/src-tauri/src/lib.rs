//! The Concordia editor shell: hosts an engine in-process for the editor UI (ADR-0004, ADR-0043).
//! The UI is just another Engine API client; this crate holds no editing logic.

pub mod bench;
#[cfg(target_os = "macos")]
pub mod native;

use std::path::Path;

use concordia_api::transport::ws::{self, WsServer};
use concordia_api::{Engine, EngineConfig, EngineHandle};
use concordia_app::Project;
use serde_json::{Value, json};

/// Why the host could not start.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// The project could not be loaded.
    #[error("project: {0}")]
    Project(String),
    /// The engine or its API server failed.
    #[error("engine: {0}")]
    Engine(String),
}

/// An engine on its own thread, serving the Engine API on localhost with a per-launch token.
pub struct EngineHost {
    handle: EngineHandle,
    runtime: tokio::runtime::Runtime,
    server: WsServer,
    base: String,
}

impl std::fmt::Debug for EngineHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineHost")
            .field("url", &self.base)
            .finish_non_exhaustive()
    }
}

impl EngineHost {
    /// Loads `project` (its modules and main scene) and starts the API server.
    pub fn start(project: &Path) -> Result<Self, HostError> {
        Self::start_with_log(project, None)
    }

    /// [`EngineHost::start`], serving `log.tail` from `log_ring` (the process's log capture).
    pub fn start_with_log(
        project: &Path,
        log_ring: Option<std::sync::Arc<concordia_core::log::LogRing>>,
    ) -> Result<Self, HostError> {
        if !project.join("concordia.toml").is_file() {
            return Err(HostError::Project(format!(
                "no concordia.toml in {}; create a project with `concordia new`",
                project.display()
            )));
        }
        let loaded = Project::load(project).map_err(|e| HostError::Project(e.to_string()))?;
        let mut config = EngineConfig::new(project);
        config.log_ring = log_ring;
        config.modules = loaded
            .modules()
            .map_err(|e| HostError::Project(e.to_string()))?;
        let engine = Engine::new(config).map_err(|e| HostError::Engine(e.message))?;
        let handle = engine.spawn();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| HostError::Engine(e.to_string()))?;
        let server = runtime
            .block_on(ws::serve(handle.clone(), ws::WsConfig::localhost(0)))
            .map_err(|e| HostError::Engine(e.message))?;
        ws::write_connection_file(project, &server)
            .map_err(|e| HostError::Engine(e.to_string()))?;
        let base = server.url().trim_end_matches('/').to_owned();
        let host = Self {
            handle,
            runtime,
            server,
            base,
        };
        if let Some(main) = loaded.manifest.scene.main {
            host.call("scene.open", json!({ "path": main }))
                .map_err(HostError::Engine)?;
        }
        Ok(host)
    }

    /// `ws://127.0.0.1:<port>` without the token.
    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// The URL the editor UI connects with (`…/?token=…`).
    pub fn url(&self) -> String {
        format!("{}/?token={}", self.base, self.server.token)
    }

    /// The engine's handle (for `run` and calls from the shell).
    pub fn handle(&self) -> &EngineHandle {
        &self.handle
    }

    /// The tokio runtime the server runs on.
    pub fn runtime(&self) -> &tokio::runtime::Runtime {
        &self.runtime
    }

    /// Calls a method on a fresh shell session (actor `tool:editor-shell`); errors as text.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.session()?.call(method, params)
    }

    /// Calls `method` once per params, in order, on one shell session.
    pub fn call_each(
        &self,
        method: &str,
        params: impl IntoIterator<Item = Value>,
    ) -> Result<Vec<Value>, String> {
        let session = self.session()?;
        params
            .into_iter()
            .map(|p| session.call(method, p))
            .collect()
    }

    /// Opens a shell session (actor `tool:editor-shell`), closed when dropped.
    pub fn session(&self) -> Result<ShellSession<'_>, String> {
        let id = self
            .runtime
            .block_on(self.handle.open_session())
            .map_err(|e| e.message)?;
        let session = ShellSession { host: self, id };
        session.call(
            "session.open",
            json!({"client": "editor-shell", "actor": "tool:editor-shell"}),
        )?;
        Ok(session)
    }
}

/// One API session of the shell; blocking calls.
pub struct ShellSession<'a> {
    host: &'a EngineHost,
    id: concordia_core::id::SessionId,
}

impl ShellSession<'_> {
    /// Calls a method on this session; errors as text.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let handle = &self.host.handle;
        self.host
            .runtime
            .block_on(handle.call(self.id, method, params))
            .map_err(|e| e.message)
    }
}

impl Drop for ShellSession<'_> {
    fn drop(&mut self) {
        self.host.handle.close_session(self.id);
    }
}
