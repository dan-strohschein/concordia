//! Typed text from key presses (M2 phase 2.7b.3): editing keys and Ctrl/Cmd shortcuts become
//! `Edit` commands; other keys insert the text the OS produced.

use concordia_input::{EditKey, KeyCode, TextInput};

/// The modifiers held with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Shift (extends a selection).
    pub shift: bool,
    /// Ctrl or Cmd (shortcuts).
    pub command: bool,
}

/// The typed input a key press makes, if any.
pub fn text_from_key(key: KeyCode, text: Option<&str>, mods: Modifiers) -> Option<TextInput> {
    let edit = |key| {
        Some(TextInput::Edit {
            key,
            select: mods.shift,
        })
    };
    if mods.command {
        return match key {
            KeyCode::A => edit(EditKey::SelectAll),
            KeyCode::C => edit(EditKey::Copy),
            KeyCode::X => edit(EditKey::Cut),
            KeyCode::V => edit(EditKey::Paste),
            _ => None,
        };
    }
    match key {
        KeyCode::ArrowLeft => edit(EditKey::Left),
        KeyCode::ArrowRight => edit(EditKey::Right),
        KeyCode::ArrowUp => edit(EditKey::Up),
        KeyCode::ArrowDown => edit(EditKey::Down),
        KeyCode::Home => edit(EditKey::Home),
        KeyCode::End => edit(EditKey::End),
        KeyCode::Backspace => edit(EditKey::Backspace),
        KeyCode::Delete => edit(EditKey::Delete),
        KeyCode::Enter => edit(EditKey::Enter),
        _ => insert(text?),
    }
}

/// `text` without control characters, as an insertion (`None` if nothing is left).
pub fn insert(text: &str) -> Option<TextInput> {
    let printable: String = text.chars().filter(|c| !c.is_control()).collect();
    (!printable.is_empty()).then_some(TextInput::Insert(printable))
}
