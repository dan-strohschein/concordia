//! Studio errors: config files, the task board and the journal.

use std::path::{Path, PathBuf};

/// A Studio file or operation failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StudioError {
    /// A Markdown file lacks valid `---` YAML frontmatter.
    #[error("{file}: {message}")]
    Frontmatter {
        /// The file (or a description of it).
        file: String,
        /// What is wrong.
        message: String,
    },
    /// The frontmatter or TOML has a key the format does not define.
    #[error("{file}: unknown key {key:?}{}", hint(did_you_mean))]
    UnknownKey {
        /// The file.
        file: String,
        /// The unknown key.
        key: String,
        /// The closest known key.
        did_you_mean: Option<String>,
    },
    /// A value is invalid.
    #[error("{file}: {message}")]
    Invalid {
        /// The file.
        file: String,
        /// What is wrong.
        message: String,
    },
    /// A role file's `role` differs from its file name.
    #[error("role {role:?} must be in {role}.md, not {stem}.md")]
    RoleStem {
        /// The `role` field.
        role: String,
        /// The file stem.
        stem: String,
    },
    /// `mcp_tools` patterns that match no tool the project offers.
    #[error(
        "{file}: mcp_tools {patterns:?} match no tool{}",
        list_hint(suggestions)
    )]
    UnknownTools {
        /// The file.
        file: String,
        /// The patterns.
        patterns: Vec<String>,
        /// Close tool names.
        suggestions: Vec<String>,
    },
    /// A path pattern escapes the project.
    #[error("{file}: path {path:?} must be a relative path inside the project")]
    PathOutsideProject {
        /// The file.
        file: String,
        /// The pattern.
        path: String,
    },
    /// The team's director role is missing or disabled.
    #[error("studio/team.toml: {message}")]
    Director {
        /// What is wrong.
        message: String,
    },
    /// A file could not be read or written.
    #[error("{}: {message}", path.display())]
    Io {
        /// The path.
        path: PathBuf,
        /// The OS error.
        message: String,
    },
    /// The Studio journal is unreadable.
    #[error("studio journal line {line}: {message}")]
    Journal {
        /// One-based line.
        line: usize,
        /// What is wrong.
        message: String,
    },
}

impl StudioError {
    /// Names the file for errors raised before it was known.
    pub fn in_file(self, name: &str) -> Self {
        match self {
            Self::Frontmatter { message, .. } => Self::Frontmatter {
                file: name.into(),
                message,
            },
            Self::UnknownKey {
                key, did_you_mean, ..
            } => Self::UnknownKey {
                file: name.into(),
                key,
                did_you_mean,
            },
            Self::Invalid { message, .. } => Self::Invalid {
                file: name.into(),
                message,
            },
            Self::UnknownTools {
                patterns,
                suggestions,
                ..
            } => Self::UnknownTools {
                file: name.into(),
                patterns,
                suggestions,
            },
            Self::PathOutsideProject { path, .. } => Self::PathOutsideProject {
                file: name.into(),
                path,
            },
            other => other,
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid {
            file: String::new(),
            message: message.into(),
        }
    }

    pub(crate) fn io(path: &Path, e: &std::io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            message: e.to_string(),
        }
    }
}

fn hint(did_you_mean: &Option<String>) -> String {
    did_you_mean
        .as_ref()
        .map(|s| format!("; did you mean {s:?}?"))
        .unwrap_or_default()
}

fn list_hint(suggestions: &[String]) -> String {
    match suggestions.is_empty() {
        true => "; see studio.roster or schema.get for tool names".into(),
        false => format!("; did you mean {}?", suggestions.join(", ")),
    }
}
