//! RHI errors.

/// Something the RHI could not do.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RhiError {
    /// No usable GPU device (none present, or the backend is unavailable on this platform).
    #[error("no GPU device: {reason}")]
    NoDevice {
        /// Why, in words an agent can act on.
        reason: String,
    },
    /// A descriptor or argument is invalid.
    #[error("invalid {what}: {reason}")]
    Invalid {
        /// What was invalid (e.g. "texture desc").
        what: &'static str,
        /// Why.
        reason: String,
    },
    /// A handle does not name a live resource of this device.
    #[error("{kind} handle {index}:{generation} is stale or unknown")]
    StaleHandle {
        /// Resource kind.
        kind: &'static str,
        /// Slot index.
        index: u32,
        /// Generation.
        generation: u32,
    },
    /// A shader or pipeline failed to compile; `log` is the compiler's message.
    #[error("pipeline {label:?} failed to compile:\n{log}")]
    PipelineCompile {
        /// Pipeline label.
        label: String,
        /// Compiler output.
        log: String,
    },
    /// The device does not support a feature (see `RhiCaps`).
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// The backend failed to allocate.
    #[error("out of memory allocating {0}")]
    OutOfMemory(String),
    /// The GPU was lost (reset, removed, or a fatal command error).
    #[error("GPU device lost: {0}")]
    DeviceLost(String),
}

impl RhiError {
    /// An `Invalid` error.
    pub fn invalid(what: &'static str, reason: impl Into<String>) -> Self {
        Self::Invalid {
            what,
            reason: reason.into(),
        }
    }
}
