//! Subcommands.

pub(crate) mod fmt;
pub(crate) mod gpu_probe;
pub(crate) mod headless;
pub(crate) mod new;
pub(crate) mod run;
pub(crate) mod schema;

use std::future::Future;

use concordia_api::ApiError;

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
