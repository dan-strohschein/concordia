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
}
