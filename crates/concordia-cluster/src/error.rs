//! Errors from building, encoding or decoding meshlets.

/// Why a meshlet operation failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ClusterError {
    /// Build settings are out of range.
    #[error("meshlet settings: {0}")]
    Settings(String),
    /// The input mesh is malformed.
    #[error("meshlet input: {0}")]
    Input(String),
    /// No meshlet size fits the device's mesh-output limits.
    #[error("mesh-output limits {vertices} vertices / {primitives} primitives fit no meshlet size")]
    Limits {
        /// Maximum vertices per mesh-shader group.
        vertices: u32,
        /// Maximum primitives per mesh-shader group.
        primitives: u32,
    },
    /// Encoded meshlets are malformed or of another format version.
    #[error("meshlet data: {0}")]
    Decode(String),
    /// meshoptimizer refused the call.
    #[error(transparent)]
    Meshopt(#[from] meshopt_sys::MeshoptError),
}
