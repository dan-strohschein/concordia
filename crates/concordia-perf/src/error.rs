//! Errors from reading perf files, probing the host and running scenes.

/// What went wrong.
#[derive(Debug, thiserror::Error)]
pub enum PerfError {
    /// A perf file is not valid.
    #[error("{path}: {message}")]
    File {
        /// The file.
        path: String,
        /// Why.
        message: String,
    },
    /// A value breaks a rule (negative budget, no frames, ...).
    #[error("{0}")]
    Invalid(String),
    /// The engine refused a call while running a scene.
    #[error("{method}: {message}")]
    Engine {
        /// The Engine API method.
        method: String,
        /// Its error.
        message: String,
    },
}
