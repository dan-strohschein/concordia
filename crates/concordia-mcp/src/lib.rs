//! The Concordia MCP server: a thin adapter that exposes every Engine API method
//! (except `session.open`) as an MCP tool with its generated schema, plus
//! schema, doc and scene resources. No API key is involved anywhere.
//!
//! rmcp-specific code lives only in `tools.rs` and `resources.rs` (ADR-0033).

mod resources;
mod tools;

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use concordia_api::{ApiError, Engine, EngineConfig, EngineHandle, EngineModule, MethodRegistry};
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
    /// Scene to open at startup (project-relative).
    pub scene: Option<String>,
    /// Seed for reproducible ids (tests and gates).
    pub deterministic_ids: Option<u64>,
    /// Job system threads (default: all cores).
    pub threads: Option<NonZeroUsize>,
    /// Log ring served by `log_tail`.
    pub log_ring: Option<Arc<LogRing>>,
    /// Engine modules after the core module; their methods become tools (ADR-0046).
    pub modules: Vec<Arc<dyn EngineModule>>,
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
        }
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
}

/// A running engine plus the session every MCP call uses.
#[derive(Debug, Clone)]
pub struct ConcordiaServer {
    handle: EngineHandle,
    session: SessionId,
}

impl ConcordiaServer {
    /// Starts an engine thread and opens a session (and the startup scene, if any).
    pub async fn start(options: Options) -> Result<Self, StartError> {
        if options.actor.is_human() {
            return Err(StartError::HumanActor(options.actor));
        }
        let mut config = EngineConfig::new(&options.project);
        config.deterministic_ids = options.deterministic_ids;
        config.log_ring = options.log_ring;
        config.modules = options.modules;
        if let Some(threads) = options.threads {
            config.threads = threads;
        }
        let handle = Engine::new(config)?.spawn();
        let session = handle.open_session().await?;
        let open = json!({ "client": "mcp", "actor": options.actor.to_string() });
        handle.call(session, "session.open", open).await?;
        if let Some(scene) = options.scene {
            handle
                .call(session, "scene.open", json!({ "path": scene }))
                .await?;
        }
        Ok(Self { handle, session })
    }

    /// Every method the engine serves; one tool each, except `session.open`.
    pub(crate) fn methods(&self) -> &MethodRegistry {
        self.handle.methods()
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
    let modules =
        concordia_app::Project::load(&args.project).and_then(|project| project.modules())?;
    let options = Options {
        project: args.project,
        actor: args.actor,
        scene: args.scene,
        deterministic_ids: args.deterministic_ids,
        threads: args.threads,
        log_ring: ring,
        modules,
    };
    let server = ConcordiaServer::start(options).await?;
    tracing::info!(target: "concordia::mcp", "MCP server ready on stdio");
    tools::serve_stdio(server).await
}
