//! Animation errors: each names the skeleton joint, clip channel, graph node or asset at fault.

use concordia_assets::{AnimGraphError, AssetError};

/// Why a skeleton, clip or pose was refused.
#[derive(Debug, thiserror::Error)]
pub enum AnimError {
    /// The skeleton's joints are inconsistent.
    #[error("skeleton: {0}")]
    Skeleton(String),
    /// A clip channel is malformed.
    #[error("clip {clip}, channel {channel} ({target}): {reason}")]
    Clip {
        /// Clip name.
        clip: String,
        /// Channel index.
        channel: usize,
        /// The channel's target bone name.
        target: String,
        /// What is wrong.
        reason: String,
    },
    /// The clip as a whole is malformed.
    #[error("clip {clip}: {reason}")]
    ClipHeader {
        /// Clip name.
        clip: String,
        /// What is wrong.
        reason: String,
    },
    /// A pose has the wrong number of bones for its skeleton.
    #[error("pose has {got} bones, the skeleton {expected}")]
    PoseSize {
        /// The skeleton's joint count.
        expected: usize,
        /// The pose's bone count.
        got: usize,
    },
    /// A mesh can't be skinned with the given palette.
    #[error("skinning: {0}")]
    Mesh(String),
    /// A graph node can't be bound (an unknown clip, say).
    #[error("graph node {node:?}: {reason}")]
    Graph {
        /// The node at fault.
        node: String,
        /// What is wrong.
        reason: String,
    },
    /// The graph itself is malformed.
    #[error(transparent)]
    GraphFile(#[from] AnimGraphError),
    /// Loading an asset failed.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Reading the world failed.
    #[error(transparent)]
    Query(#[from] concordia_terra::QueryError),
}
