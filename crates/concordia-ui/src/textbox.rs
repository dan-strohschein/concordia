//! Text entry (2.7b.3): `UiTextBox` (single- and multi-line, selection, clipboard, `max_len`,
//! password masking), `UiSpinBox` (a number kept in a text box on the same entity, with step
//! arrows), `UiList` (its children are items) and `edit`, which applies one typed input.
//! Byte offsets always sit on character boundaries. Godot's `LineEdit`, `TextEdit`, `SpinBox`
//! and `ItemList`, and WAI-ARIA's textbox and listbox patterns, for semantics.

use concordia_input::{EditKey, TextInput};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::{UiError, finite};
use crate::widgets::Wrap;

/// The most text a box holds, in bytes.
pub const MAX_BOX_BYTES: usize = 64 * 1024;
/// The most a `max_len` may be.
pub const MAX_LEN: u32 = 65536;
/// The most list items selected at once.
pub const MAX_SELECTED: usize = 4096;
/// The mask a password box draws for each character.
pub const MASK: char = '•';

/// An editable text field. The engine moves `caret` and `anchor` (byte offsets; the selection
/// lies between them) as you type and click.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiTextBox {
    /// The text.
    pub text: String,
    /// Shown, muted, while the text is empty.
    pub placeholder: String,
    /// Enter breaks lines instead of submitting.
    pub multiline: bool,
    /// The most characters it takes.
    pub max_len: Option<u32>,
    /// Draws a mask for each character (the text itself stays real).
    pub password: bool,
    /// Typing changes it (else it only selects and copies).
    pub editable: bool,
    /// The caret, a byte offset.
    pub caret: u32,
    /// The selection's other end, a byte offset (equal to `caret` when nothing is selected).
    pub anchor: u32,
    /// How a multi-line box wraps.
    pub wrap: Wrap,
}

impl Default for UiTextBox {
    fn default() -> Self {
        Self {
            text: String::new(),
            placeholder: String::new(),
            multiline: false,
            max_len: None,
            password: false,
            editable: true,
            caret: 0,
            anchor: 0,
            wrap: Wrap::Word,
        }
    }
}

impl UiTextBox {
    /// Checks the sizes, the offsets and that a single-line box has no newline.
    pub fn check(&self) -> Result<(), UiError> {
        const NAME: &str = "UiTextBox";
        if self.text.len() > MAX_BOX_BYTES {
            return Err(UiError::field(NAME, "text", "at most 64 KiB"));
        }
        if !self.multiline && self.text.contains('\n') {
            return Err(UiError::field(
                NAME,
                "text",
                "a single-line box holds no newline",
            ));
        }
        if self.placeholder.len() > 4096 {
            return Err(UiError::field(NAME, "placeholder", "at most 4096 bytes"));
        }
        if self.max_len.is_some_and(|m| m > MAX_LEN) {
            return Err(UiError::field(NAME, "max_len", "at most 65536"));
        }
        for (field, at) in [("caret", self.caret), ("anchor", self.anchor)] {
            if !self.text.is_char_boundary(at as usize) {
                return Err(UiError::field(
                    NAME,
                    field,
                    format!("{at} is not a character boundary of the text"),
                ));
            }
        }
        Ok(())
    }

    /// The selected byte range (empty when nothing is selected).
    pub fn selection(&self) -> std::ops::Range<usize> {
        let (a, b) = (self.caret as usize, self.anchor as usize);
        a.min(b)..a.max(b)
    }

    /// What it draws: the text, or a mask per character for a password box.
    pub fn shown(&self) -> String {
        match self.password {
            true => self.text.chars().map(|_| MASK).collect(),
            false => self.text.clone(),
        }
    }

    /// Byte offset `at` of the text in the drawn text (they differ for a password box).
    pub fn shown_offset(&self, at: usize) -> usize {
        match self.password {
            true => self.text[..at.min(self.text.len())].chars().count() * MASK.len_utf8(),
            false => at,
        }
    }

    /// The text offset of drawn offset `at` (the inverse of `shown_offset`).
    pub fn text_offset(&self, at: usize) -> usize {
        match self.password {
            true => self
                .text
                .char_indices()
                .map(|(i, _)| i)
                .chain(std::iter::once(self.text.len()))
                .nth(at / MASK.len_utf8())
                .unwrap_or(self.text.len()),
            false => at.min(self.text.len()),
        }
    }
}

/// A number typed or stepped; needs a `UiTextBox` on the same entity, which shows the value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiSpinBox {
    /// The value.
    pub value: f32,
    /// The low end.
    pub min: f32,
    /// The high end.
    pub max: f32,
    /// What the arrows and keys add.
    pub step: f32,
    /// Digits after the point.
    #[reflect(range(min = 0.0, max = 6.0))]
    pub decimals: u32,
}

impl Default for UiSpinBox {
    fn default() -> Self {
        Self {
            value: 0.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            decimals: 0,
        }
    }
}

impl UiSpinBox {
    /// Checks finite values, min < max, a positive step and at most 6 decimals.
    pub fn check(&self) -> Result<(), UiError> {
        const NAME: &str = "UiSpinBox";
        finite(NAME, "value", &[self.value])?;
        finite(NAME, "min", &[self.min])?;
        finite(NAME, "max", &[self.max])?;
        if self.min >= self.max {
            return Err(UiError::field(NAME, "max", "must be above min"));
        }
        if !(self.step.is_finite() && self.step > 0.0) {
            return Err(UiError::field(NAME, "step", "must be positive"));
        }
        match self.decimals <= 6 {
            true => Ok(()),
            false => Err(UiError::field(NAME, "decimals", "at most 6")),
        }
    }

    /// `v` clamped to min..max and rounded to the decimals.
    pub fn fit(&self, v: f32) -> f32 {
        let scale = 10f32.powi(self.decimals as i32);
        ((v.clamp(self.min, self.max) * scale).round() / scale).clamp(self.min, self.max)
    }

    /// The value as text.
    pub fn format(&self, v: f32) -> String {
        format!("{v:.*}", self.decimals as usize)
    }
}

/// A list of items (its children); click selects, Ctrl adds, Shift selects a range.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiList {
    /// Selected item indices, ascending.
    pub selected: Vec<u32>,
    /// More than one may be selected.
    pub multi: bool,
}

impl UiList {
    /// Checks ascending, unique indices, one at most unless `multi`.
    pub fn check(&self) -> Result<(), UiError> {
        let ascending = self.selected.windows(2).all(|w| w[0] < w[1]);
        let count_ok = self.multi || self.selected.len() <= 1;
        match ascending && count_ok && self.selected.len() <= MAX_SELECTED {
            true => Ok(()),
            false => Err(UiError::field(
                "UiList",
                "selected",
                "ascending indices without repeats, one at most unless multi",
            )),
        }
    }
}

/// The text a box changed to (signal).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "UI")]
pub struct UiTextChanged {
    /// The new text.
    pub text: String,
}

/// Enter in a single-line box (signal).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "UI")]
pub struct UiTextSubmitted {
    /// The text submitted.
    pub text: String,
}

/// The engine's clipboard (a play-world resource): copies and cuts write it, pastes read it.
/// The windowed player mirrors it to the OS clipboard.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiClipboard {
    /// The text.
    pub text: String,
    /// Bumped on every write.
    pub generation: u64,
}

/// What one edit did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EditResult {
    /// The text changed.
    pub changed: bool,
    /// Enter in a single-line box.
    pub submitted: bool,
}

/// Applies one typed input to `b`.
pub fn edit(b: &mut UiTextBox, input: &TextInput, clip: &mut UiClipboard) -> EditResult {
    let mut out = EditResult::default();
    match input {
        TextInput::Insert(s) => out.changed = insert(b, s),
        TextInput::Edit { key, select } => match key {
            EditKey::Left
            | EditKey::Right
            | EditKey::Home
            | EditKey::End
            | EditKey::Up
            | EditKey::Down => step(b, *key, *select),
            EditKey::Backspace | EditKey::Delete => out.changed = delete(b, *key),
            EditKey::SelectAll => {
                b.anchor = 0;
                b.caret = b.text.len() as u32;
            }
            EditKey::Copy => {
                copy(b, clip);
            }
            EditKey::Cut => {
                if b.editable && copy(b, clip) {
                    out.changed = replace(b, "");
                }
            }
            EditKey::Paste => out.changed = insert(b, &clip.text.clone()),
            EditKey::Enter if b.multiline => out.changed = insert(b, "\n"),
            EditKey::Enter => out.submitted = true,
        },
    }
    out
}

/// Inserts `s` over the selection, cut to `max_len`; single-line boxes turn newlines to spaces.
fn insert(b: &mut UiTextBox, s: &str) -> bool {
    if !b.editable || s.is_empty() {
        return false;
    }
    let s: String = match b.multiline {
        true => s.to_owned(),
        false => s.replace(['\n', '\r'], " "),
    };
    let sel = b.selection();
    let kept = b.text.chars().count() - b.text[sel.clone()].chars().count();
    let room = b
        .max_len
        .map_or(usize::MAX, |m| (m as usize).saturating_sub(kept));
    let s: String = s.chars().take(room).collect();
    if s.is_empty() && sel.is_empty() {
        return false;
    }
    replace(b, &s)
}

/// Replaces the selection with `s` and puts the caret after it.
fn replace(b: &mut UiTextBox, s: &str) -> bool {
    let sel = b.selection();
    if sel.is_empty() && s.is_empty() {
        return false;
    }
    b.text.replace_range(sel.clone(), s);
    let at = (sel.start + s.len()) as u32;
    b.caret = at;
    b.anchor = at;
    true
}

/// Deletes the selection, else the character before (Backspace) or after (Delete) the caret.
fn delete(b: &mut UiTextBox, key: EditKey) -> bool {
    if !b.editable {
        return false;
    }
    if b.selection().is_empty() {
        let c = b.caret as usize;
        let other = match key {
            EditKey::Backspace => prev(&b.text, c),
            _ => next(&b.text, c),
        };
        b.anchor = other as u32;
    }
    replace(b, "")
}

/// Copies the selection unless it is a password box; true when it copied.
fn copy(b: &UiTextBox, clip: &mut UiClipboard) -> bool {
    let sel = b.selection();
    if b.password || sel.is_empty() {
        return false;
    }
    clip.text = b.text[sel].to_owned();
    clip.generation += 1;
    true
}

/// Moves the caret (extending the selection when `select`).
fn step(b: &mut UiTextBox, key: EditKey, select: bool) {
    let sel = b.selection();
    let c = b.caret as usize;
    let to = match key {
        EditKey::Left if !select && !sel.is_empty() => sel.start,
        EditKey::Right if !select && !sel.is_empty() => sel.end,
        EditKey::Left => prev(&b.text, c),
        EditKey::Right => next(&b.text, c),
        EditKey::Home => line_start(&b.text, c),
        EditKey::End => line_end(&b.text, c),
        EditKey::Up | EditKey::Down if !b.multiline => c,
        EditKey::Up => vertical(&b.text, c, false),
        EditKey::Down => vertical(&b.text, c, true),
        _ => c,
    };
    b.caret = to as u32;
    if !select {
        b.anchor = b.caret;
    }
}

/// The character boundary before `at`.
fn prev(text: &str, at: usize) -> usize {
    text[..at].char_indices().next_back().map_or(0, |(i, _)| i)
}

/// The character boundary after `at`.
fn next(text: &str, at: usize) -> usize {
    text[at..].chars().next().map_or(at, |c| at + c.len_utf8())
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |i| at + i)
}

/// The same column (in characters) on the line below (`down`) or above.
fn vertical(text: &str, at: usize, down: bool) -> usize {
    let start = line_start(text, at);
    let column = text[start..at].chars().count();
    let target = if down {
        let end = line_end(text, at);
        if end == text.len() {
            return at;
        }
        end + 1
    } else {
        if start == 0 {
            return at;
        }
        line_start(text, start - 1)
    };
    let end = line_end(text, target);
    text[target..end]
        .char_indices()
        .nth(column)
        .map_or(end, |(i, _)| target + i)
}
