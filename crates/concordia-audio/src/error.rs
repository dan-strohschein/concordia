//! Audio errors.

use thiserror::Error;

/// Something about audio data, a layout or a request is wrong.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum AudioError {
    /// A source file could not be decoded.
    #[error("cannot decode {format}: {reason}")]
    Decode {
        /// `wav` or `ogg`.
        format: &'static str,
        /// What went wrong.
        reason: String,
    },
    /// Sample data is malformed.
    #[error("invalid sound: {0}")]
    Sound(String),
    /// Two buses share a name.
    #[error("bus {0:?} is defined twice")]
    DuplicateBus(String),
    /// A bus sends to a bus that doesn't exist.
    #[error("bus {bus:?} sends to unknown bus {send:?}")]
    UnknownSend {
        /// The bus at fault.
        bus: String,
        /// The missing parent.
        send: String,
    },
    /// Sends form a cycle.
    #[error("buses form a cycle: {}", path.join(" -> "))]
    BusCycle {
        /// The cycle, starting and ending at the same bus.
        path: Vec<String>,
    },
    /// A bus field is out of range, or Master has a send.
    #[error("bus {bus:?}: {reason}")]
    Bus {
        /// The bus at fault.
        bus: String,
        /// What is wrong.
        reason: String,
    },
    /// A mixer setting is out of range.
    #[error("mixer config: {0}")]
    Config(String),
    /// A play request is malformed.
    #[error("play request: {0}")]
    Request(String),
    /// The command queue is full (the audio thread is not draining it).
    #[error("the audio command queue is full")]
    QueueFull,
    /// A text document failed to parse.
    #[error("{0}")]
    Parse(String),
}

/// A `.cue.ron` problem, naming its field.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CueError {
    /// A field is out of range.
    #[error("cue field {field}: {reason}")]
    Field {
        /// The JSON-pointer-like field name (`sounds[0].weight`).
        field: String,
        /// What is wrong.
        reason: String,
    },
    /// The text failed to parse.
    #[error("{0}")]
    Parse(String),
}
