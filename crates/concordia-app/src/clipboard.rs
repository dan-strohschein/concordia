//! The OS clipboard for windowed play (M2 phase 2.7b.3): pastes read it, the UI's copies
//! (`ui.clipboard`) are written to it. Behind a trait, so play can be tested without one.

/// A clipboard the player reads and writes.
pub trait Clipboard: Send {
    /// Its text, if it holds any.
    fn get(&mut self) -> Option<String>;
    /// Replaces its text.
    fn set(&mut self, text: &str);
}

/// The OS clipboard (arboard), opened on first use; a failure is warned about once and the
/// clipboard reads as empty.
#[derive(Default)]
pub struct OsClipboard {
    board: Option<arboard::Clipboard>,
    failed: bool,
}

impl OsClipboard {
    fn board(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.board.is_none() && !self.failed {
            match arboard::Clipboard::new() {
                Ok(board) => self.board = Some(board),
                Err(e) => {
                    self.failed = true;
                    tracing::warn!(target: "concordia::app", error = %e, "no OS clipboard; copy and paste stay inside the game");
                }
            }
        }
        self.board.as_mut()
    }
}

impl Clipboard for OsClipboard {
    fn get(&mut self) -> Option<String> {
        self.board()?.get_text().ok().filter(|t| !t.is_empty())
    }

    fn set(&mut self, text: &str) {
        if let Some(board) = self.board()
            && let Err(e) = board.set_text(text.to_owned())
        {
            tracing::warn!(target: "concordia::app", error = %e, "couldn't write the OS clipboard");
        }
    }
}

/// `text` cut to at most `max` bytes on a character boundary.
pub(crate) fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
