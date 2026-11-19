//! `studio/team.toml`: who is on the team, limits and approval gates.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::error::StudioError;
use crate::gate::Gate;
use crate::role::RoleId;
use crate::scope::check_project_pattern;

/// The whole team file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Team {
    /// `[team]`.
    pub team: TeamSection,
    /// `[roles.<id>]` overrides.
    #[serde(default)]
    pub roles: BTreeMap<String, RoleOverride>,
    /// `[concurrency]`.
    #[serde(default)]
    pub concurrency: Concurrency,
    /// `[budgets]`.
    #[serde(default)]
    pub budgets: Budgets,
    /// `[[gates]]`.
    #[serde(default)]
    pub gates: Vec<Gate>,
    /// `[escalation]`.
    #[serde(default)]
    pub escalation: Escalation,
    /// `[paths]`.
    #[serde(default)]
    pub paths: TeamPaths,
}

/// `[team]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamSection {
    /// Team name.
    pub name: String,
    /// The role the main session runs as.
    pub director: RoleId,
    /// Role for hook calls with no agent type (default: the director).
    #[serde(default)]
    pub main_session_actor: Option<RoleId>,
}

/// `[roles.<id>]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleOverride {
    /// Instances (overrides the role file).
    #[serde(default)]
    pub count: Option<NonZeroU32>,
    /// False removes the role from the roster.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Why the role is disabled (shown in `studio.roster`).
    #[serde(default)]
    pub reason: String,
}

fn yes() -> bool {
    true
}

/// `[concurrency]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Concurrency {
    /// Subagents running at once.
    pub max_parallel_subagents: NonZeroU32,
    /// Spawn depth (1: only the Director spawns).
    pub max_spawn_depth: NonZeroU32,
}

impl Default for Concurrency {
    fn default() -> Self {
        Self {
            max_parallel_subagents: NonZeroU32::new(4).unwrap_or(NonZeroU32::MIN),
            max_spawn_depth: NonZeroU32::MIN,
        }
    }
}

/// `[budgets]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Budgets {
    /// Turn limit per subagent run (a role's lower value wins).
    pub max_turns_per_subagent: Option<NonZeroU32>,
    /// Engine MCP calls per task (enforced by `concordia mcp`, phase 3.5).
    pub max_mcp_calls_per_task: Option<NonZeroU32>,
    /// Engine mutations per task (phase 3.5).
    pub max_mutations_per_task: Option<NonZeroU32>,
    /// Passed as `--max-budget-usd` when the engine launches `claude -p` (phase 3.8).
    pub headless_max_budget_usd: Option<f64>,
}

/// `[escalation]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Escalation {
    /// Rejections before a task is blocked and handed to a human.
    pub max_attempts_per_task: NonZeroU32,
}

impl Default for Escalation {
    fn default() -> Self {
        Self {
            max_attempts_per_task: NonZeroU32::new(3).unwrap_or(NonZeroU32::MIN),
        }
    }
}

/// `[paths]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TeamPaths {
    /// No agent may read or write these.
    pub deny: Vec<String>,
    /// Write paths more than one role may own.
    pub shared: Vec<String>,
}

impl Team {
    /// Parses and checks a team file.
    pub fn parse(text: &str) -> Result<Self, StudioError> {
        let team: Self = toml::from_str(text).map_err(|e| StudioError::Invalid {
            file: "studio/team.toml".into(),
            message: e.to_string(),
        })?;
        team.paths
            .deny
            .iter()
            .chain(&team.paths.shared)
            .try_for_each(|p| check_project_pattern(p))
            .map_err(|e| e.in_file("studio/team.toml"))?;
        Ok(team)
    }

    /// The role hook calls without an agent type act as.
    pub fn main_session_actor(&self) -> &RoleId {
        self.team
            .main_session_actor
            .as_ref()
            .unwrap_or(&self.team.director)
    }
}
