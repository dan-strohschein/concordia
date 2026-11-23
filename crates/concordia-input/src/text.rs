//! Typed text: what a text field receives (M2 phase 2.7b.3). Insertions and editing commands
//! queue as `InputEvent::Text`, come out of sampling as the tick's [`TypedText`], and replays
//! record them, so typing replays exactly.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::InputError;

/// The longest insertion, in bytes.
pub const MAX_INSERT_BYTES: usize = 4096;

/// An editing command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum EditKey {
    /// One character back.
    Left,
    /// One character on.
    Right,
    /// The line above.
    Up,
    /// The line below.
    Down,
    /// The line's start.
    Home,
    /// The line's end.
    End,
    /// Delete back.
    Backspace,
    /// Delete on.
    Delete,
    /// Select everything.
    SelectAll,
    /// Copy the selection to the clipboard.
    Copy,
    /// Cut the selection to the clipboard.
    Cut,
    /// Insert the clipboard.
    Paste,
    /// Submit, or a new line when multi-line.
    Enter,
}

/// One piece of typed input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum TextInput {
    /// Text to insert (typed characters or a paste).
    Insert(String),
    /// An editing command; `select` extends the selection (Shift).
    Edit {
        /// The command.
        key: EditKey,
        /// Extend the selection instead of moving it.
        select: bool,
    },
}

impl TextInput {
    /// Checks an insertion: 1..=4096 bytes, no control characters but newline and tab.
    pub fn check(&self) -> Result<(), InputError> {
        let TextInput::Insert(text) = self else {
            return Ok(());
        };
        let printable = text
            .chars()
            .all(|c| !c.is_control() || c == '\n' || c == '\t');
        match (1..=MAX_INSERT_BYTES).contains(&text.len()) && printable {
            true => Ok(()),
            false => Err(InputError::Pointer(format!(
                "typed text must be 1..={MAX_INSERT_BYTES} bytes without control characters \
                 (newline and tab allowed); got {} bytes",
                text.len()
            ))),
        }
    }
}

/// The text typed this tick, in order; a play-world resource published by `input.sample`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TypedText(pub Vec<TextInput>);
