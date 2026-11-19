//! The Concordia editor shell: joins the project's shared engine for the editor UI (ADR-0004,
//! ADR-0043, ADR-0082). It hosts the engine in-process (and publishes it) unless one is already
//! live, in which case it attaches. The UI is just another Engine API client; this crate holds no
//! editing logic.

pub mod bench;
pub mod claude_launcher;
pub mod input;
pub mod launcher_commands;
#[cfg(target_os = "macos")]
pub mod native;

use std::path::Path;

use concordia_api::transport::connection::HostKind;
use concordia_api::{ApiErrorKind, EngineConfig, EngineHandle};
use concordia_app::{Project, SharedEngine};
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

/// The project's engine as the editor sees it: hosted here, or attached to another process's.
pub struct EngineHost {
    engine: SharedEngine,
    runtime: tokio::runtime::Runtime,
    base: String,
}

impl std::fmt::Debug for EngineHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineHost")
            .field("url", &self.base)
            .field("host", &self.engine.is_host())
            .finish_non_exhaustive()
    }
}

impl EngineHost {
    /// Loads `project`, attaches to its live engine or hosts one, and opens the main scene if the
    /// engine has none open.
    pub fn start(project: &Path) -> Result<Self, HostError> {
        Self::start_with_log(project, None)
    }

    /// [`EngineHost::start`], serving `log.tail` from `log_ring` when hosting (the process's log capture).
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
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| HostError::Engine(e.to_string()))?;
        let engine = runtime
            .block_on(SharedEngine::attach_or_host(config, HostKind::Editor))
            .map_err(|e| HostError::Engine(e.to_string()))?;
        let base = engine.info().url.trim_end_matches('/').to_owned();
        let host = Self {
            engine,
            runtime,
            base,
        };
        if let Some(main) = loaded.manifest.scene.main {
            host.open_main_scene(&main).map_err(HostError::Engine)?;
        }
        Ok(host)
    }

    /// Opens `main` unless the shared engine already has a scene open.
    fn open_main_scene(&self, main: &str) -> Result<(), String> {
        let session = self.session()?;
        let info =
            self.runtime
                .block_on(self.engine.link().call(session.id, "scene.info", json!({})));
        match info {
            Err(e) if e.kind == ApiErrorKind::NoSceneOpen => session
                .call("scene.open", json!({ "path": main }))
                .map(drop),
            Err(e) => Err(e.message),
            Ok(_) => Ok(()),
        }
    }

    /// True when this editor hosts the engine (and can drive the native viewport).
    pub fn is_host(&self) -> bool {
        self.engine.is_host()
    }

    /// Who hosts the engine, e.g. "concordia mcp, pid 4242".
    pub fn host_description(&self) -> String {
        let info = self.engine.info();
        format!("{}, pid {}", info.host, info.pid)
    }

    /// `ws://127.0.0.1:<port>` without the token.
    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// The URL the editor UI connects with (`…/?token=…`).
    pub fn url(&self) -> String {
        self.engine.info().connect_url()
    }

    /// The in-process engine, when this editor hosts it (for `run`: the native viewport).
    pub fn handle(&self) -> Option<&EngineHandle> {
        self.engine.handle()
    }

    /// The tokio runtime the engine connection runs on.
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
        self.session_as("tool:editor-shell")
    }

    /// Opens a session acting as `actor` (e.g. `human:local` for the user's own input).
    pub fn session_as(&self, actor: &str) -> Result<ShellSession<'_>, String> {
        let id = self
            .runtime
            .block_on(self.engine.link().open_session())
            .map_err(|e| e.message)?;
        let session = ShellSession { host: self, id };
        session.call(
            "session.open",
            json!({"client": "editor-shell", "actor": actor}),
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
        let link = self.host.engine.link();
        self.host
            .runtime
            .block_on(link.call(self.id, method, params))
            .map_err(|e| e.message)
    }
}

impl Drop for ShellSession<'_> {
    fn drop(&mut self) {
        self.host.engine.link().close_session(self.id);
    }
}
