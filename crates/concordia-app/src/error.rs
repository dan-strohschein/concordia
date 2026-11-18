//! Project errors.

use std::path::PathBuf;

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
}
