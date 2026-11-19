//! Subcommands.

pub(crate) mod doctor;
pub(crate) mod fmt;
pub(crate) mod gpu_probe;
pub(crate) mod headless;
pub(crate) mod import;
pub(crate) mod new;
pub(crate) mod run;
pub(crate) mod schema;
pub(crate) mod studio;

use std::future::Future;

use std::path::Path;

use concordia_api::{ApiError, Engine, EngineConfig};

use crate::Failure;

/// Runs an async subcommand on a fresh multi-threaded runtime.
pub(crate) fn block_on(task: impl Future<Output = Result<(), Failure>>) -> Result<(), Failure> {
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
    let project = concordia_app::Project::load(root).map_err(|e| Failure::usage(e.to_string()))?;
    let mut config = EngineConfig::new(root);
    config.modules = project
        .modules()
        .map_err(|e| Failure::usage(e.to_string()))?;
    Engine::new(config).map_err(|e| Failure::usage(e.message))
}
