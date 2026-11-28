//! Fracture errors.

use manifold_sys::ManifoldError;

/// Why a mesh couldn't be fractured.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FractureError {
    /// The mesh is empty, has an index out of range or a value that isn't finite.
    #[error("invalid mesh: {0}")]
    InvalidMesh(String),
    /// The mesh isn't closed and manifold, so it has no inside to cut.
    #[error("the mesh isn't closed and manifold")]
    NotClosed,
    /// The pattern's parameters are out of range.
    #[error("invalid pattern: {0}")]
    InvalidPattern(String),
    /// A parameter is out of range.
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
    /// The piece's collision hull couldn't be built.
    #[error("no collision hull: {0}")]
    Hull(String),
    /// Reading or writing the derived-data cache failed.
    #[error("derived-data cache: {0}")]
    Io(String),
    /// The cutter failed.
    #[error("the cutter failed")]
    Cutter,
}

impl From<ManifoldError> for FractureError {
    fn from(e: ManifoldError) -> Self {
        match e {
            ManifoldError::Invalid(why) => Self::InvalidMesh(why),
            ManifoldError::NotManifold => Self::NotClosed,
            ManifoldError::Failed => Self::Cutter,
        }
    }
}
