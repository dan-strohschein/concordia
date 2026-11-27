//! Errors reading and writing graphs.

/// What went wrong with a graph document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    /// The text isn't a valid graph (line, column and pointer in the message).
    #[error("graph: {0}")]
    Parse(String),
    /// An id isn't a UUIDv7.
    #[error("{what} {id:?} is not a UUIDv7")]
    InvalidId {
        /// What the id names (the graph, a node, a layout entry, a link).
        what: &'static str,
        /// The id.
        id: String,
    },
}
