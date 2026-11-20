//! Subcommands.

pub(crate) mod doctor;
pub(crate) mod fmt;
pub(crate) mod gpu_probe;
pub(crate) mod headless;
pub(crate) mod import;
pub(crate) mod new;
pub(crate) mod replay;
pub(crate) mod run;
pub(crate) mod schema;
pub(crate) mod studio;

use std::future::Future;

use std::path::Path;

use concordia_api::{ApiError, Engine};

use crate::Failure;

/// Runs an async subcommand on a fresh multi-threaded runtime.
pub(crate) fn block_on<T>(task: impl Future<Output = Result<T, Failure>>) -> Result<T, Failure> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| Failure::usage(format!("cannot start the async runtime: {e}")))?;
    runtime.block_on(task)
}

/// An API error as a CLI failure: message plus suggestions.
pub(crate) fn api_failure(error: &ApiError) -> Failure {
    let suggestions: String = error
        .data
        .suggestions
        .iter()
        .map(|s| format!("\n  - {s}"))
        .collect();
    Failure::usage(format!("{}{suggestions}", error.message))
}

/// An in-process engine with the project's modules.
pub(crate) fn project_engine(root: &Path) -> Result<Engine, Failure> {
    project_engine_on(root, None)
}

/// [`project_engine`] with `threads` job threads (default: the engine's).
pub(crate) fn project_engine_on(
    root: &Path,
    threads: Option<std::num::NonZeroUsize>,
) -> Result<Engine, Failure> {
    let mut config = concordia_app::Project::load(root)
        .and_then(|p| p.engine_config())
        .map_err(|e| Failure::usage(e.to_string()))?;
    if let Some(threads) = threads {
        config.threads = threads;
    }
    Engine::new(config).map_err(|e| Failure::usage(e.message))
}
