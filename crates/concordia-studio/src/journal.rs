//! The Studio journal (ADR-0080): `.concordia/studio/journal.jsonl`, append-only, one entry per line.
//!
//! The engine and out-of-process hooks both append, each holding an exclusive lock on the file
//! while they write. An entry's `seq` is its line number, assigned on read.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use concordia_command::Actor;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::StudioError;

/// Current journal format version.
pub const STUDIO_JOURNAL_VERSION: u32 = 1;

/// Where the journal lives, relative to the project root.
pub const STUDIO_JOURNAL_PATH: &str = ".concordia/studio/journal.jsonl";

/// A field's value before and after a change.
pub type Change = [Value; 2];

/// What happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JournalEvent {
    /// A task was created.
    TaskCreated {
        /// Task id.
        task: String,
        /// Title.
        title: String,
        /// Assigned role.
        role: String,
    },
    /// Task fields changed (status, assignee, priority, ...).
    TaskChanged {
        /// Task id.
        task: String,
        /// Field → [before, after].
        changes: BTreeMap<String, Change>,
    },
    /// A note was added to a task.
    TaskComment {
        /// Task id.
        task: String,
        /// The note.
        text: String,
    },
    /// A reviewer approved or rejected a task.
    TaskReview {
        /// Task id.
        task: String,
        /// True for approve.
        approved: bool,
        /// The reviewer's comment.
        comment: String,
        /// Field → [before, after].
        changes: BTreeMap<String, Change>,
    },
    /// A file was edited by an agent tool (written by the PostToolUse hook).
    FileEdit {
        /// Project-relative path.
        path: String,
        /// The harness tool (`Edit`, `Write`, ...).
        tool: String,
        /// Content hash before (none: new file).
        before: Option<String>,
        /// Content hash after (none: deleted).
        after: Option<String>,
    },
    /// `concordia studio sync` regenerated the harness files.
    Sync {
        /// Files written.
        #[serde(default)]
        changed: Vec<String>,
    },
    /// A team member asked a question (ADR-0081).
    QuestionAsked {
        /// Question id (`Q-0001`).
        question: String,
        /// Addressee: a role id or `human`.
        to: String,
        /// The question.
        text: String,
        /// The task it is about.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task: Option<String>,
        /// Related ids (tasks, questions).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        refs: Vec<String>,
    },
    /// A question was answered (the first answer closes it).
    QuestionAnswered {
        /// Question id.
        question: String,
        /// The answer.
        text: String,
    },
    /// The director escalated a question to the human.
    QuestionEscalated {
        /// Question id.
        question: String,
        /// Why no one on the team can answer.
        reason: String,
    },
}

impl JournalEvent {
    /// The task this event is about, if any.
    pub fn task(&self) -> Option<&str> {
        match self {
            Self::TaskCreated { task, .. }
            | Self::TaskChanged { task, .. }
            | Self::TaskComment { task, .. }
            | Self::TaskReview { task, .. } => Some(task),
            Self::QuestionAsked { task, .. } => task.as_deref(),
            Self::FileEdit { .. }
            | Self::Sync { .. }
            | Self::QuestionAnswered { .. }
            | Self::QuestionEscalated { .. } => None,
        }
    }

    /// The `kind` tag.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::TaskCreated { .. } => "task_created",
            Self::TaskChanged { .. } => "task_changed",
            Self::TaskComment { .. } => "task_comment",
            Self::TaskReview { .. } => "task_review",
            Self::FileEdit { .. } => "file_edit",
            Self::Sync { .. } => "sync",
            Self::QuestionAsked { .. } => "question_asked",
            Self::QuestionAnswered { .. } => "question_answered",
            Self::QuestionEscalated { .. } => "question_escalated",
        }
    }
}

/// One journal entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Line number (1-based), assigned on read.
    #[serde(skip_serializing, default)]
    pub seq: u64,
    /// Format version.
    pub v: u32,
    /// Unix milliseconds.
    pub ts_ms: u64,
    /// Who.
    pub actor: Actor,
    /// What.
    #[serde(flatten)]
    pub event: JournalEvent,
}

impl JournalEntry {
    /// The entry as API JSON, with its `seq`.
    pub fn to_json(&self) -> Value {
        let mut value = serde_json::to_value(self).unwrap_or(Value::Null);
        value["seq"] = self.seq.into();
        value
    }
}

/// A handle on a Studio journal file.
#[derive(Debug, Clone)]
pub struct StudioJournal {
    path: PathBuf,
}

impl StudioJournal {
    /// The journal at `path` (created on first append).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The project's journal.
    pub fn of_project(root: &Path) -> Self {
        Self::new(root.join(STUDIO_JOURNAL_PATH))
    }

    /// The file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends one entry under an exclusive lock, dropping any incomplete last line first.
    pub fn append(&self, actor: &Actor, event: JournalEvent) -> Result<(), StudioError> {
        let entry = JournalEntry {
            seq: 0,
            v: STUDIO_JOURNAL_VERSION,
            ts_ms: now_ms(),
            actor: actor.clone(),
            event,
        };
        let mut line =
            serde_json::to_vec(&entry).map_err(|e| StudioError::invalid(e.to_string()))?;
        line.push(b'\n');
        let mut file = self.open_locked()?;
        cut_partial_tail(&mut file).map_err(|e| StudioError::io(&self.path, &e))?;
        let written = file.write_all(&line).and_then(|()| file.sync_data());
        let _ = file.unlock();
        written.map_err(|e| StudioError::io(&self.path, &e))
    }

    /// Entries after `since` (all when none); an incomplete last line is ignored.
    pub fn read(&self, since: Option<u64>) -> Result<Vec<JournalEntry>, StudioError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(StudioError::io(&self.path, &e)),
        };
        let since = since.unwrap_or(0);
        complete_lines(&text)
            .enumerate()
            .map(|(i, line)| (i as u64 + 1, line))
            .filter(|(seq, _)| *seq > since)
            .map(|(seq, line)| parse_line(seq, line))
            .collect()
    }

    /// The last complete entry's `seq` (0 when empty).
    pub fn last_seq(&self) -> Result<u64, StudioError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => Ok(complete_lines(&text).count() as u64),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(StudioError::io(&self.path, &e)),
        }
    }

    fn open_locked(&self) -> Result<File, StudioError> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| StudioError::io(dir, &e))?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)
            .map_err(|e| StudioError::io(&self.path, &e))?;
        file.lock().map_err(|e| StudioError::io(&self.path, &e))?;
        Ok(file)
    }
}

/// Lines that end with a newline (a crash can leave the last one incomplete).
fn complete_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split_inclusive('\n')
        .filter(|l| l.ends_with('\n'))
        .map(|l| l.trim_end_matches('\n'))
}

/// Truncates the file after its last newline and leaves the cursor at the end.
fn cut_partial_tail(file: &mut File) -> std::io::Result<()> {
    if file.seek(SeekFrom::End(0))? == 0 {
        return Ok(());
    }
    let mut last = [0u8];
    file.seek(SeekFrom::End(-1))?;
    file.read_exact(&mut last)?;
    if last[0] == b'\n' {
        return Ok(());
    }
    let mut text = Vec::new();
    file.seek(SeekFrom::Start(0))?;
    file.read_to_end(&mut text)?;
    let keep = text.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    if keep != text.len() {
        file.set_len(keep as u64)?;
    }
    file.seek(SeekFrom::End(0))?;
    Ok(())
}

fn parse_line(seq: u64, line: &str) -> Result<JournalEntry, StudioError> {
    let error = |message: String| StudioError::Journal {
        line: seq as usize,
        message,
    };
    let value: Value = serde_json::from_str(line).map_err(|e| error(e.to_string()))?;
    let version = value.get("v").and_then(Value::as_u64).unwrap_or(0);
    if version != u64::from(STUDIO_JOURNAL_VERSION) {
        return Err(error(format!(
            "format version {version}; this engine reads version {STUDIO_JOURNAL_VERSION}"
        )));
    }
    let mut entry: JournalEntry =
        serde_json::from_value(value).map_err(|e| error(e.to_string()))?;
    entry.seq = seq;
    Ok(entry)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
