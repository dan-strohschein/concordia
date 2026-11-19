//! Tasks: ids, statuses, priorities and the `.task.md` file format.

use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::StudioError;
use crate::frontmatter;

/// A task id: `T-0001`, sequential per project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(NonZeroU32);

impl TaskId {
    /// The id numbered `n`.
    pub fn new(n: NonZeroU32) -> Self {
        Self(n)
    }

    /// The number.
    pub fn number(self) -> NonZeroU32 {
        self.0
    }

    /// The next id.
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "T-{:04}", self.0)
    }
}

impl FromStr for TaskId {
    type Err = StudioError;
    fn from_str(s: &str) -> Result<Self, StudioError> {
        s.strip_prefix("T-")
            .filter(|n| n.len() >= 4 && n.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|n| n.parse::<NonZeroU32>().ok())
            .map(Self)
            .ok_or_else(|| StudioError::invalid(format!("task id {s:?} must look like T-0001")))
    }
}

/// Where a task is in its life.
#[allow(non_camel_case_types)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
pub enum TaskStatus {
    /// Not started.
    todo,
    /// Claimed and being worked on.
    in_progress,
    /// Waiting for review.
    in_review,
    /// Approved.
    done,
    /// Needs a human.
    blocked,
    /// Sent back by a reviewer.
    rejected,
}

impl TaskStatus {
    /// Every status, in board order.
    pub const ALL: [Self; 6] = [
        Self::todo,
        Self::in_progress,
        Self::in_review,
        Self::done,
        Self::blocked,
        Self::rejected,
    ];

    /// The wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::todo => "todo",
            Self::in_progress => "in_progress",
            Self::in_review => "in_review",
            Self::done => "done",
            Self::blocked => "blocked",
            Self::rejected => "rejected",
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskStatus {
    type Err = StudioError;
    fn from_str(s: &str) -> Result<Self, StudioError> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == s)
            .ok_or_else(|| {
                StudioError::invalid(format!(
                    "task status {s:?} must be one of todo, in_progress, in_review, done, blocked, rejected"
                ))
            })
    }
}

/// How urgent a task is.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Priority {
    /// Low.
    low,
    /// Normal (default).
    #[default]
    normal,
    /// High.
    high,
    /// Urgent.
    urgent,
}

/// One acceptance criterion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Acceptance {
    /// What must be true, in words (checked by the reviewer).
    pub assert: String,
    /// A playtest suite that proves it (M3 phase 3.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playtest: Option<String>,
    /// A replay that proves it (M3 phase 3.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<String>,
}

/// A task file's frontmatter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskMeta {
    /// `T-0001`.
    pub id: String,
    /// Globally unique id (UUIDv7).
    pub uid: String,
    /// Title.
    pub title: String,
    /// Status.
    pub status: TaskStatus,
    /// The role that does it.
    pub role: String,
    /// Who holds it (an actor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// Priority.
    #[serde(default)]
    pub priority: Priority,
    /// Tasks that must be done first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// Who created it (an actor).
    pub created_by: String,
    /// Acceptance criteria.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance: Vec<Acceptance>,
    /// Related links or paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
}

const TASK_KEYS: [&str; 11] = [
    "id",
    "uid",
    "title",
    "status",
    "role",
    "assignee",
    "priority",
    "depends_on",
    "created_by",
    "acceptance",
    "links",
];

const NOTES: &str = "## Notes";
const REVIEW: &str = "## Review";

/// A task: frontmatter, description and the notes and review lines the API appends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Frontmatter.
    pub meta: TaskMeta,
    /// Description (Markdown).
    pub description: String,
    /// `## Notes` lines.
    pub notes: Vec<String>,
    /// `## Review` lines.
    pub review: Vec<String>,
}

impl Task {
    /// Parses a `.task.md` file.
    pub fn parse(text: &str) -> Result<Self, StudioError> {
        let (yaml, body) = frontmatter::split(text)?;
        let meta: TaskMeta = frontmatter::parse(yaml, &TASK_KEYS)?;
        meta.id.parse::<TaskId>()?;
        let (description, after) = section(body, NOTES);
        let (notes, review) = match after {
            Some(after) => {
                let (notes, review) = section(after, REVIEW);
                (lines(notes), review.map(lines).unwrap_or_default())
            }
            None => (Vec::new(), Vec::new()),
        };
        Ok(Self {
            meta,
            description: description.trim_end().to_owned(),
            notes,
            review,
        })
    }

    /// The canonical text.
    pub fn to_text(&self) -> String {
        let block = |items: &[String]| match items.is_empty() {
            true => String::new(),
            false => format!("{}\n", items.join("\n")),
        };
        let body = format!(
            "{}\n\n{NOTES}\n{}\n{REVIEW}\n{}",
            self.description.trim_end(),
            block(&self.notes),
            block(&self.review),
        );
        frontmatter::join(&self.meta, &body)
    }

    /// The id.
    pub fn id(&self) -> Result<TaskId, StudioError> {
        self.meta.id.parse()
    }
}

/// Splits `body` at a heading line: (before, after the heading) or (body, None).
fn section<'a>(body: &'a str, heading: &str) -> (&'a str, Option<&'a str>) {
    let marker = format!("\n{heading}\n");
    match (
        body.strip_prefix(&format!("{heading}\n")),
        body.find(&marker),
    ) {
        (Some(after), _) => ("", Some(after)),
        (None, Some(i)) => (&body[..i], Some(&body[i + marker.len()..])),
        (None, None) => (body, None),
    }
}

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A file-name slug from a title: lowercase words joined by `-`, at most 40 characters.
pub fn slug(title: &str) -> String {
    let words: Vec<String> = title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let mut slug = String::new();
    for word in words {
        if !slug.is_empty() && slug.len() + 1 + word.len() > 40 {
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(&word[..word.len().min(40)]);
    }
    match slug.is_empty() {
        true => "task".into(),
        false => slug,
    }
}
