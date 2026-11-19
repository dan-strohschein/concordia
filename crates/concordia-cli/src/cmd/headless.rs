//! `concordia headless`.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use concordia_api::transport::connection::{ConnectionInfo, HostKind};
use concordia_api::transport::stdio;
use concordia_api::{Engine, EngineConfig};
use concordia_app::SharedEngine;
use concordia_core::log::{LogConfig, init};

use super::api_failure;
use crate::Failure;

/// `concordia headless` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Project root.
    #[arg(long, default_value = ".")]
    project: PathBuf,
    /// Serve JSON-RPC over WebSocket on localhost instead of stdio.
    #[arg(long)]
    ws: bool,
    /// WebSocket port (0 picks a free one).
    #[arg(long, default_value_t = 0, requires = "ws")]
    port: u16,
    /// Reproducible ids from this seed (tests only).
    #[arg(long)]
    deterministic_ids: Option<u64>,
    /// Job system threads.
    #[arg(long)]
    threads: Option<NonZeroUsize>,
}

pub(crate) async fn run(args: Args) -> Result<(), Failure> {
    let log = init(&LogConfig::from_env()).map_err(|e| Failure::usage(e.to_string()))?;
    let project =
        concordia_app::Project::load(&args.project).map_err(|e| Failure::usage(e.to_string()))?;
    let mut config = EngineConfig::new(&args.project);
    config.modules = project
        .modules()
        .map_err(|e| Failure::usage(e.to_string()))?;
    config.deterministic_ids = args.deterministic_ids;
    config.log_ring = Some(Arc::clone(log.ring()));
    if let Some(threads) = args.threads {
        config.threads = threads;
    }
    if !args.ws {
        let handle = Engine::new(config).map_err(|e| api_failure(&e))?.spawn();
        let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
        return stdio::serve(&handle, stdin, stdout)
            .await
            .map_err(|e| Failure::usage(e.to_string()));
    }
    let engine = SharedEngine::attach_or_host_on(config, HostKind::Headless, args.port)
        .await
        .map_err(|e| Failure::usage(e.to_string()))?;
    let info = engine.info();
    if !engine.is_host() {
        return Err(Failure::usage(format!(
            "an engine is already running for this project: {} (pid {}) at {}; connect to it (see .concordia/headless.json) or stop it first",
            info.host, info.pid, info.url
        )));
    }
    eprintln!(
        "concordia headless: {} (also in {})",
        info.connect_url(),
        ConnectionInfo::path(&args.project).display()
    );
    shutdown_signal().await?;
    Ok(())
}

/// Ctrl-C, or SIGTERM on Unix; the hosted engine then removes its connection file.
async fn shutdown_signal() -> Result<(), Failure> {
    let failure = |e: std::io::Error| Failure::usage(e.to_string());
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).map_err(failure)?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result.map_err(failure),
            _ = term.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.map_err(failure)
    }
}
