//! Errors from building scenes and rendering.

/// What went wrong.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PathTraceError {
    /// A mesh is malformed (indices, non-finite positions).
    #[error("mesh: {0}")]
    Mesh(String),
    /// A material does not compile.
    #[error("material: {0}")]
    Material(String),
    /// An environment map is malformed.
    #[error("environment: {0}")]
    Environment(String),
    /// A texture is malformed.
    #[error("texture: {0}")]
    Texture(String),
    /// The scene has no camera.
    #[error("the scene has no camera")]
    NoCamera,
    /// The GPU backend failed (device, shader or submission).
    #[error("gpu: {0}")]
    Gpu(String),
    /// Render settings are out of range.
    #[error("settings: {0}")]
    Settings(String),
}
