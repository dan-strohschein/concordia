//! Approval gates: who may make a task transition or touch protected paths.

use std::fmt;
use std::str::FromStr;

use concordia_command::{Actor, ActorKind};
use serde::{Deserialize, Serialize};

use crate::error::StudioError;
use crate::role::RoleId;
use crate::task::TaskStatus;

/// A party a gate accepts: `human` (any `human:*`) or `role:<id>` (any instance of that role).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum GateParty {
    /// Any human actor.
    Human,
    /// Any `agent:<role>[#n]` actor.
    Role(RoleId),
}

impl GateParty {
    /// True if `actor` is this party.
    pub fn admits(&self, actor: &Actor) -> bool {
        match self {
            Self::Human => actor.is_human(),
            Self::Role(role) => actor.kind() == ActorKind::Agent && actor.name() == role.as_str(),
        }
    }
}

impl fmt::Display for GateParty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Human => f.write_str("human"),
            Self::Role(role) => write!(f, "role:{role}"),
        }
    }
}

impl FromStr for GateParty {
    type Err = StudioError;
    fn from_str(s: &str) -> Result<Self, StudioError> {
        match (s, s.strip_prefix("role:")) {
            ("human", _) => Ok(Self::Human),
            (_, Some(role)) => RoleId::new(role).map(Self::Role),
            _ => Err(StudioError::invalid(format!(
                "gate party {s:?} must be \"human\" or \"role:<id>\""
            ))),
        }
    }
}

impl TryFrom<String> for GateParty {
    type Error = StudioError;
    fn try_from(s: String) -> Result<Self, StudioError> {
        s.parse()
    }
}

impl From<GateParty> for String {
    fn from(p: GateParty) -> Self {
        p.to_string()
    }
}

/// What a gate guards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawOn", into = "RawOn")]
pub enum GateOn {
    /// A task status change, e.g. `in_review->done`.
    TaskTransition {
        /// From.
        from: TaskStatus,
        /// To.
        to: TaskStatus,
    },
    /// Writes to these project paths (globs).
    Paths(Vec<String>),
}

/// The TOML form: `{ task_transition = "a->b" }` or `{ paths = [...] }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    task_transition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paths: Option<Vec<String>>,
}

impl TryFrom<RawOn> for GateOn {
    type Error = StudioError;
    fn try_from(raw: RawOn) -> Result<Self, StudioError> {
        match (raw.task_transition, raw.paths) {
            (Some(t), None) => parse_transition(&t),
            (None, Some(paths)) => Ok(Self::Paths(paths)),
            _ => Err(StudioError::invalid(
                "a gate's `on` has exactly one of task_transition or paths",
            )),
        }
    }
}

impl From<GateOn> for RawOn {
    fn from(on: GateOn) -> Self {
        match on {
            GateOn::TaskTransition { from, to } => Self {
                task_transition: Some(format!("{}->{}", from.as_str(), to.as_str())),
                paths: None,
            },
            GateOn::Paths(paths) => Self {
                task_transition: None,
                paths: Some(paths),
            },
        }
    }
}

fn parse_transition(text: &str) -> Result<GateOn, StudioError> {
    let (from, to) = text.split_once("->").ok_or_else(|| {
        StudioError::invalid(format!(
            "task_transition {text:?} must be <from>-><to>, e.g. in_review->done"
        ))
    })?;
    Ok(GateOn::TaskTransition {
        from: from.trim().parse()?,
        to: to.trim().parse()?,
    })
}

/// A gate: what it guards and who may pass it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    /// What it guards.
    pub on: GateOn,
    /// Any one of these may pass.
    pub require_any: Vec<GateParty>,
}
