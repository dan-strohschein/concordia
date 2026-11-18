//! `concordia headless`.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use concordia_api::transport::{stdio, ws};
use concordia_api::{Engine, EngineConfig};
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
    let mut config = EngineConfig::new(&args.project);
    config.deterministic_ids = args.deterministic_ids;
    config.log_ring = Some(Arc::clone(log.ring()));
    if let Some(threads) = args.threads {
        config.threads = threads;
    }
    let handle = Engine::new(config).map_err(|e| api_failure(&e))?.spawn();
    if !args.ws {
        let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
        return stdio::serve(&handle, stdin, stdout)
            .await
            .map_err(|e| Failure::usage(e.to_string()));
    }
    let server = ws::serve(handle, ws::WsConfig::localhost(args.port))
        .await
        .map_err(|e| api_failure(&e))?;
    let file = ws::write_connection_file(&args.project, &server)
        .map_err(|e| Failure::usage(e.to_string()))?;
    eprintln!(
        "concordia headless: {}?token={} (also in {})",
        server.url(),
        server.token,
        file.display()
    );
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| Failure::usage(e.to_string()))?;
    Ok(())
}
