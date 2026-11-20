//! Replay file errors.

use concordia_scene::SceneError;

/// A replay file could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    /// The text did not parse or match the schema.
    #[error("replay: {0}")]
    Parse(#[from] SceneError),
    /// The file is another format version.
    #[error(
        "replay format version {found} is not supported; this engine reads version {}",
        crate::FORMAT_VERSION
    )]
    Version {
        /// The version in the file.
        found: u32,
    },
    /// A hash is not 64 hex digits.
    #[error("replay {at}: {value:?} is not a world hash (64 hex digits)")]
    Hash {
        /// Where, e.g. `checkpoints[2].parts.Transform`.
        at: String,
        /// The value.
        value: String,
    },
    /// Ticks are not in increasing order, or past the recorded length.
    #[error("replay {at}: tick {tick} is out of order (ticks increase and end at {ticks})")]
    Order {
        /// Where, e.g. `inputs[3]`.
        at: String,
        /// The tick.
        tick: u64,
        /// The recorded length.
        ticks: u64,
    },
}
