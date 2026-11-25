//! Project errors.

use std::path::PathBuf;

use concordia_api::ApiError;
use concordia_api::transport::connection::HostKind;

/// A project could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// `concordia.toml` is unreadable or invalid.
    #[error("{path}: {message}")]
    Manifest {
        /// The manifest path.
        path: PathBuf,
        /// What is wrong.
        message: String,
    },
    /// A file the manifest names is unreadable or invalid.
    #[error("{path}: {message}")]
    File {
        /// The file.
        path: PathBuf,
        /// What is wrong.
        message: String,
    },
    /// `CONCORDIA_BACKEND` names no backend.
    #[error("{0}")]
    Backend(#[from] concordia_rhi_select::SelectError),
    /// The engine could not start.
    #[error("engine: {0}")]
    Engine(#[from] ApiError),
    /// A live engine for this project runs another Concordia version (ADR-0082).
    #[error(
        "{host} (pid {pid}) runs the engine for this project with Concordia {theirs}, but this is {ours}; close it or use the same version"
    )]
    VersionMismatch {
        /// The host's pid.
        pid: u32,
        /// The kind of host.
        host: HostKind,
        /// The host's version.
        theirs: String,
        /// This process's version.
        ours: String,
    },
}
