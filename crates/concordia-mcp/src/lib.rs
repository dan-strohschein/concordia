//! The Concordia MCP server: a thin adapter that exposes every Engine API method
//! (except `session.open`) as an MCP tool with its generated schema, plus
//! schema, doc and scene resources. No API key is involved anywhere.
//!
//! rmcp-specific code lives only in `tools.rs` and `resources.rs` (ADR-0033).

mod resources;
mod studio;
mod tools;

pub use tools::is_tool;

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use concordia_api::transport::connection::HostKind;
use concordia_api::{
    ApiError, ApiErrorKind, EngineConfig, EngineLink, EngineModule, MethodRegistry,
};
use concordia_app::{AppError, SharedEngine};
use concordia_command::Actor;
use concordia_core::id::SessionId;
use concordia_core::log::{LogConfig, LogError, LogRing};
use serde_json::json;

/// Server options.
#[derive(Debug, Clone)]
pub struct Options {
    /// Project root.
    pub project: PathBuf,
    /// Default actor for every tool call (never `human:*`).
    pub actor: Actor,
    /// Scene to open at startup (project-relative) unless the shared engine already has one open.
    pub scene: Option<String>,
    /// Seed for reproducible ids when this server hosts the engine (tests and gates).
    pub deterministic_ids: Option<u64>,
    /// Job system threads when this server hosts the engine (default: all cores).
    pub threads: Option<NonZeroUsize>,
    /// Log ring served by `log_tail` when this server hosts the engine.
    pub log_ring: Option<Arc<LogRing>>,
    /// Engine modules after the core module; their methods become tools (ADR-0046).
    pub modules: Vec<Arc<dyn EngineModule>>,
    /// Agent Studio folder (project-relative) when `[studio]` is on: per-agent sessions and scopes (ADR-0080).
    pub studio: Option<PathBuf>,
}

impl Options {
    /// Defaults for `project`, acting as `agent:claude-code`.
    pub fn new(project: impl AsRef<Path>) -> Self {
        Self {
            project: project.as_ref().to_owned(),
            actor: "agent:claude-code".parse().expect("valid literal actor"),
            scene: None,
            deterministic_ids: None,
            threads: None,
            log_ring: None,
            modules: Vec::new(),
            studio: None,
        }
    }

    fn engine_config(&self) -> EngineConfig {
        let mut config = EngineConfig::new(&self.project);
        config.deterministic_ids = self.deterministic_ids;
        config.log_ring = self.log_ring.clone();
        config.modules = self.modules.clone();
        if let Some(threads) = self.threads {
            config.threads = threads;
        }
        config
    }
}

/// The server could not start.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// `human:*` actors may not act over MCP.
    #[error(
        "{0} is a human actor; MCP clients act as agent:<role>, e.g. --actor agent:claude-code"
    )]
    HumanActor(Actor),
    /// The engine rejected the configuration or the startup scene.
    #[error("engine: {0}")]
    Engine(#[from] ApiError),
    /// The project's shared engine could not be attached or hosted (ADR-0082).
    #[error("{0}")]
    Shared(#[from] AppError),
}

/// The project's shared engine plus the session plain (unstamped) tool calls use.
#[derive(Debug)]
pub(crate) struct Connection {
    engine: SharedEngine,
    base: SessionId,
}

impl Connection {
    pub(crate) fn link(&self) -> &dyn EngineLink {
        self.engine.link()
    }

    pub(crate) fn base(&self) -> SessionId {
        self.base
    }
}

/// The MCP server: tools over the project's shared engine (ADR-0080, ADR-0082).
#[derive(Debug, Clone)]
pub struct ConcordiaServer {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    options: Options,
    methods: MethodRegistry,
    connection: tokio::sync::Mutex<Option<Arc<Connection>>>,
    studio: Option<studio::StudioGate>,
}

impl ConcordiaServer {
    /// Attaches to the project's live engine or hosts one (publishing `.concordia/headless.json`),
    /// then opens a session (and the startup scene, if any).
    pub async fn start(options: Options) -> Result<Self, StartError> {
        if options.actor.is_human() {
            return Err(StartError::HumanActor(options.actor));
        }
        let methods = MethodRegistry::with_core(&options.modules)?;
        let connection = connect(&options).await?;
        let studio = options
            .studio
            .as_ref()
            .map(|dir| studio::StudioGate::new(&options.project, dir));
        Ok(Self {
            inner: Arc::new(Inner {
                options,
                methods,
                connection: tokio::sync::Mutex::new(Some(Arc::new(connection))),
                studio,
            }),
        })
    }

    /// Every method the engine serves; one tool each, except `session.open`.
    pub(crate) fn methods(&self) -> &MethodRegistry {
        &self.inner.methods
    }

    pub(crate) fn studio(&self) -> Option<&studio::StudioGate> {
        self.inner.studio.as_ref()
    }

    /// The current connection; after the host exited, attaches or hosts again.
    pub(crate) async fn connection(&self) -> Result<Arc<Connection>, ApiError> {
        let mut current = self.inner.connection.lock().await;
        if let Some(connection) = current.as_ref() {
            return Ok(Arc::clone(connection));
        }
        let connection = Arc::new(
            connect(&self.inner.options)
                .await
                .map_err(reconnect_error)?,
        );
        tracing::info!(
            target: "concordia::mcp",
            host = %connection.engine.info().host,
            pid = connection.engine.info().pid,
            "reconnected to the project's engine"
        );
        *current = Some(Arc::clone(&connection));
        Ok(connection)
    }

    /// Drops `connection` after its host exited, so the next call reconnects.
    pub(crate) async fn forget_if_exited(&self, connection: &Arc<Connection>, error: &ApiError) {
        if error.kind != ApiErrorKind::EngineExited {
            return;
        }
        let mut current = self.inner.connection.lock().await;
        if current.as_ref().is_some_and(|c| Arc::ptr_eq(c, connection)) {
            *current = None;
            if let Some(gate) = self.studio() {
                gate.forget_sessions();
            }
        }
    }
}

/// Attaches or hosts, opens the base session, and opens the startup scene if none is open.
async fn connect(options: &Options) -> Result<Connection, StartError> {
    let engine = SharedEngine::attach_or_host(options.engine_config(), HostKind::Mcp).await?;
    let link = engine.link();
    let base = link.open_session().await?;
    let open = json!({ "client": "mcp", "actor": options.actor.to_string() });
    link.call(base, "session.open", open).await?;
    if let Some(scene) = &options.scene {
        let none_open = matches!(
            link.call(base, "scene.info", json!({})).await,
            Err(e) if e.kind == ApiErrorKind::NoSceneOpen
        );
        if none_open {
            link.call(base, "scene.open", json!({ "path": scene }))
                .await?;
        }
    }
    Ok(Connection { engine, base })
}

fn reconnect_error(error: StartError) -> ApiError {
    match error {
        StartError::Engine(e) => e,
        other => ApiError::new(ApiErrorKind::EngineExited, other.to_string()),
    }
}

/// Command-line arguments shared by `concordia-mcp` and `concordia mcp`.
#[derive(Debug, Clone, clap::Args)]
pub struct Args {
    /// Project root.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
    /// Default actor for tool calls (agent:<role>; human:* is rejected).
    #[arg(long, default_value = "agent:claude-code")]
    pub actor: Actor,
    /// Scene to open at startup (project-relative path).
    #[arg(long)]
    pub scene: Option<String>,
    /// Reproducible ids from this seed (tests only).
    #[arg(long)]
    pub deterministic_ids: Option<u64>,
    /// Job system threads.
    #[arg(long)]
    pub threads: Option<NonZeroUsize>,
}

/// Runs the server on stdio until the client disconnects, with the project's modules. Logs go to stderr only.
pub async fn run(args: Args) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ring = match concordia_core::log::init(&LogConfig::from_env()) {
        Ok(handle) => Some(Arc::clone(handle.ring())),
        Err(LogError::AlreadyInitialized) => None,
        Err(e) => return Err(e.into()),
    };
    let project = concordia_app::Project::load(&args.project)?;
    let modules = project.modules()?;
    let studio = project.manifest.studio.map(|s| PathBuf::from(s.dir));
    let options = Options {
        project: args.project,
        actor: args.actor,
        scene: args.scene,
        deterministic_ids: args.deterministic_ids,
        threads: args.threads,
        log_ring: ring,
        modules,
        studio,
    };
    let server = ConcordiaServer::start(options).await?;
    tracing::info!(target: "concordia::mcp", "MCP server ready on stdio");
    tools::serve_stdio(server).await
}
