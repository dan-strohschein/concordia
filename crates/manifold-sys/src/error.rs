//! Errors from the checked wrapper.

/// Clipping failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifoldError {
    /// The input is malformed (an index out of range, a value not finite, a zero normal).
    #[error("invalid input: {0}")]
    Invalid(String),
    /// The mesh isn't closed and manifold.
    #[error("the mesh isn't closed and manifold")]
    NotManifold,
    /// Manifold itself failed.
    #[error("Manifold failed")]
    Failed,
}
