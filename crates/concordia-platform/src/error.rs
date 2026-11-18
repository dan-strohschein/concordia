//! Platform errors.

/// A window or watcher could not be created or run.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// The OS event loop failed (or is not on the main thread on macOS).
    #[error("event loop: {0}")]
    EventLoop(String),
    /// The window could not be created.
    #[error("window: {0}")]
    Window(String),
    /// The file watcher could not start.
    #[error("file watcher on {path}: {message}")]
    Watch {
        /// Watched root.
        path: String,
        /// Why.
        message: String,
    },
}
