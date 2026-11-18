//! The append-only command journal: one JSON object per line, versioned with `v`.

use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use concordia_core::id::{SessionId, TxnId};
use concordia_ecs::WorldHash;
use serde::{Deserialize, Serialize};

use crate::actor::Actor;
use crate::op::AppliedOp;

/// Current journal format version.
pub const JOURNAL_VERSION: u32 = 1;

/// One journal line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Format version.
    pub v: u32,
    /// Monotonic sequence number, from 1.
    pub seq: u64,
    /// What happened.
    #[serde(flatten)]
    pub event: Event,
    /// The API session, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionId>,
    /// Wall-clock time in Unix milliseconds (audit only; never used by replay).
    pub ts_ms: u64,
}

/// A journaled event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// A committed transaction.
    Commit {
        /// Transaction id.
        txn: TxnId,
        /// Author.
        actor: Actor,
        /// Label.
        label: String,
        /// Ops with their inverses.
        ops: Vec<AppliedOp>,
        /// World hash before.
        hash_before: WorldHash,
        /// World hash after.
        hash_after: WorldHash,
    },
    /// An undo of `target`.
    Undo {
        /// The undone transaction.
        target: TxnId,
        /// Who undid it.
        actor: Actor,
        /// World hash after.
        hash_after: WorldHash,
    },
    /// A redo of `target`.
    Redo {
        /// The redone transaction.
        target: TxnId,
        /// Who redid it.
        actor: Actor,
        /// World hash after.
        hash_after: WorldHash,
    },
    /// An aborted transaction (audit only; the world was restored).
    Abort {
        /// Transaction id.
        txn: TxnId,
        /// Its holder.
        actor: Actor,
        /// Its label.
        label: String,
        /// Why (`aborted`, `op_failed`, `session_closed`, `timeout`, ...).
        reason: String,
    },
    /// The scene was saved.
    Save {
        /// Who saved.
        actor: Actor,
        /// Project-relative scene path.
        path: String,
        /// World hash at save.
        hash: WorldHash,
    },
}

impl Event {
    /// The world hash this event leaves behind, if it records one.
    pub fn hash_after(&self) -> Option<WorldHash> {
        match self {
            Self::Commit { hash_after, .. }
            | Self::Undo { hash_after, .. }
            | Self::Redo { hash_after, .. } => Some(*hash_after),
            Self::Save { hash, .. } => Some(*hash),
            Self::Abort { .. } => None,
        }
    }
}

/// Journal I/O or format failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JournalError {
    /// File system error.
    #[error("journal {path}: {message}")]
    Io {
        /// The journal file.
        path: PathBuf,
        /// The OS error.
        message: String,
    },
    /// A complete line is not a valid entry.
    #[error("journal line {line} is corrupt: {message}")]
    Corrupt {
        /// One-based line number.
        line: usize,
        /// Parser message.
        message: String,
    },
    /// The line uses a newer format.
    #[error(
        "journal line {line} has format version {found}; this engine reads version {JOURNAL_VERSION}"
    )]
    UnsupportedVersion {
        /// One-based line number.
        line: usize,
        /// The version found.
        found: u64,
    },
}

/// Something recoverable found while loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalWarning {
    /// The last line was incomplete (crash mid-write) and was ignored.
    TruncatedTail {
        /// One-based line number.
        line: usize,
    },
}

/// Entries read from a journal file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Loaded {
    /// Complete entries, in order.
    pub entries: Vec<JournalEntry>,
    /// Recoverable problems.
    pub warnings: Vec<JournalWarning>,
    valid_len: u64,
    needs_newline: bool,
}

/// Journal options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalConfig {
    /// `fsync` after every entry (default true).
    pub sync: bool,
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self { sync: true }
    }
}

/// An open journal file, appended to by `CommandLog`.
#[derive(Debug)]
pub struct Journal {
    file: File,
    path: PathBuf,
    sync: bool,
}

impl Journal {
    /// Reads a journal; a missing file is empty.
    pub fn load(path: &Path) -> Result<Loaded, JournalError> {
        match std::fs::read_to_string(path) {
            Ok(text) => parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Loaded::default()),
            Err(e) => Err(io(path, &e)),
        }
    }

    /// Opens (creating directories and the file) for appending, cutting any truncated tail.
    pub fn open(path: &Path, config: JournalConfig) -> Result<(Self, Loaded), JournalError> {
        let loaded = Self::load(path)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| io(path, &e))?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| io(path, &e))?;
        file.set_len(loaded.valid_len).map_err(|e| io(path, &e))?;
        file.seek(SeekFrom::End(0)).map_err(|e| io(path, &e))?;
        let mut journal = Self {
            file,
            path: path.to_owned(),
            sync: config.sync,
        };
        if loaded.needs_newline {
            journal.write(b"\n")?;
        }
        Ok((journal, loaded))
    }

    /// The file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends one entry as a line.
    pub fn append(&mut self, entry: &JournalEntry) -> Result<(), JournalError> {
        let mut line = serde_json::to_vec(entry).map_err(|e| JournalError::Corrupt {
            line: 0,
            message: e.to_string(),
        })?;
        line.push(b'\n');
        self.write(&line)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), JournalError> {
        self.file.write_all(bytes).map_err(|e| io(&self.path, &e))?;
        if self.sync {
            self.file.sync_data().map_err(|e| io(&self.path, &e))?;
        }
        Ok(())
    }
}

fn io(path: &Path, e: &std::io::Error) -> JournalError {
    JournalError::Io {
        path: path.to_owned(),
        message: e.to_string(),
    }
}

fn parse(text: &str) -> Result<Loaded, JournalError> {
    let mut loaded = Loaded::default();
    let mut offset = 0usize;
    let pieces: Vec<&str> = text.split_inclusive('\n').collect();
    for (i, piece) in pieces.iter().enumerate() {
        let line = i + 1;
        let is_last = i + 1 == pieces.len();
        let content = piece.trim_end_matches('\n');
        if content.trim().is_empty() {
            offset += piece.len();
            continue;
        }
        match parse_line(content, line) {
            Ok(entry) => loaded.entries.push(entry),
            Err(LineError::Partial) if is_last => {
                loaded.warnings.push(JournalWarning::TruncatedTail { line });
                break;
            }
            Err(LineError::Partial) => {
                return Err(JournalError::Corrupt {
                    line,
                    message: "not valid JSON".into(),
                });
            }
            Err(LineError::Fatal(e)) => return Err(e),
        }
        offset += piece.len();
        loaded.needs_newline = !piece.ends_with('\n');
    }
    loaded.valid_len = offset as u64;
    Ok(loaded)
}

enum LineError {
    Partial,
    Fatal(JournalError),
}

fn parse_line(content: &str, line: usize) -> Result<JournalEntry, LineError> {
    let value: serde_json::Value = serde_json::from_str(content).map_err(|_| LineError::Partial)?;
    let version = value
        .get("v")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if version != u64::from(JOURNAL_VERSION) {
        return Err(LineError::Fatal(JournalError::UnsupportedVersion {
            line,
            found: version,
        }));
    }
    serde_json::from_value(value).map_err(|e| {
        LineError::Fatal(JournalError::Corrupt {
            line,
            message: e.to_string(),
        })
    })
}
