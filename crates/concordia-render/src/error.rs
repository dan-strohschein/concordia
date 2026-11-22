//! Renderer errors.

use concordia_core::id::EntityId;

/// Something the renderer cannot do.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// No entity has a `Camera`.
    #[error("the world has no Camera; spawn an entity with Transform and Camera")]
    NoCamera,
    /// The requested entity has no `Camera`.
    #[error("entity {0} has no Camera component")]
    UnknownCamera(EntityId),
    /// Camera parameters are out of range.
    #[error("invalid camera: {0}")]
    InvalidCamera(String),
    /// A `Representation`'s distances are out of order or not finite.
    #[error("invalid Representation: {0}")]
    InvalidRepresentation(String),
    /// Capture size out of range.
    #[error("invalid size {width}×{height}: each side must be 1..={max}")]
    InvalidSize {
        /// Width.
        width: u32,
        /// Height.
        height: u32,
        /// Largest side.
        max: u32,
    },
    /// An asset failed to load.
    #[error("asset {reference}: {message}")]
    Asset {
        /// The reference.
        reference: String,
        /// Why.
        message: String,
    },
    /// Shader compilation failed.
    #[error("shader: {0}")]
    Shader(String),
    /// The GPU device failed.
    #[error("gpu: {0}")]
    Gpu(#[from] concordia_rhi::RhiError),
    /// The render graph failed to compile.
    #[error("render graph: {0}")]
    Graph(#[from] crate::graph::GraphError),
}
