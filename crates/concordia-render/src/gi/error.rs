//! Errors from building the GI scene.

/// What went wrong building GI's view of the scene.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GiError {
    /// A mesh couldn't be simplified into a proxy.
    #[error("proxy of {mesh}: {reason}")]
    Proxy {
        /// The mesh.
        mesh: String,
        /// Why.
        reason: String,
    },
}
