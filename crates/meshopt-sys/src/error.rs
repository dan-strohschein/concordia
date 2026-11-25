//! Why the checked wrapper refused a call.

/// An input meshoptimizer would reject (by assertion) or read out of bounds.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MeshoptError {
    /// The index count isn't a multiple of 3.
    #[error("index count {0} is not a multiple of 3")]
    IndexCount(usize),
    /// An index points past the vertex array.
    #[error("index {index} is out of range for {vertices} vertices")]
    IndexRange {
        /// The offending index.
        index: u32,
        /// The number of vertices.
        vertices: usize,
    },
    /// `max_vertices` is outside 3..=256.
    #[error("max_vertices {0} is outside 3..=256")]
    MaxVertices(usize),
    /// `max_triangles` is outside 1..=512.
    #[error("max_triangles {0} is outside 1..=512")]
    MaxTriangles(usize),
    /// `cone_weight` isn't a finite value in 0..=1.
    #[error("cone_weight {0} is outside 0..=1")]
    ConeWeight(f32),
    /// A position has a NaN or infinite coordinate.
    #[error("position {0} is not finite")]
    NonFinitePosition(usize),
    /// A meshlet's local data is malformed.
    #[error("meshlet: {0}")]
    Meshlet(String),
}
