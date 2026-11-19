//! The enforcement policy: who may call which engine tools, write which paths and run which commands.
//!
//! Built from the roster, written by `studio sync` to `.claude/concordia/policy.json` (committed),
//! and read by the hooks without starting an engine. `concordia mcp` builds the same policy from
//! the engine, so both layers enforce identical rules (ADR-0080).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobBuilder, GlobMatcher};
use serde::{Deserialize, Serialize};

use crate::error::StudioError;
use crate::gate::{GateOn, GateParty};
use crate::role::HarnessTool;
use crate::roster::Roster;
use crate::scope::ToolCatalog;

/// Where sync writes the policy, relative to the project root.
pub const POLICY_PATH: &str = ".claude/concordia/policy.json";

/// One role's limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolePolicy {
    /// Engine tools it may call.
    pub tools: BTreeSet<String>,
    /// Project paths it may change.
    pub write: Vec<String>,
    /// Allowed shell patterns; `None` means the role has no shell.
    pub bash: Option<Vec<String>>,
    /// Instances.
    pub count: u32,
}

/// Paths only some parties may change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathGate {
    /// Guarded paths.
    pub paths: Vec<String>,
    /// Roles admitted (humans always are).
    pub roles: Vec<String>,
}

/// Per-task limits enforced by `concordia mcp`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskBudgets {
    /// Engine calls per task.
    pub calls: Option<u32>,
    /// Mutating engine calls per task.
    pub mutations: Option<u32>,
}

/// The whole policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// The Studio folder, project-relative.
    pub studio: String,
    /// Hash of the `studio/` sources it was built from.
    pub source_hash: String,
    /// The role hook calls without an agent type act as.
    pub main_session_role: String,
    /// Enabled roles.
    pub roles: BTreeMap<String, RolePolicy>,
    /// Paths no agent may touch.
    pub deny: Vec<String>,
    /// Gated paths.
    pub path_gates: Vec<PathGate>,
    /// Tools that only read (all an unknown agent may call).
    pub read_only_tools: BTreeSet<String>,
    /// Per-task budgets.
    pub budgets: TaskBudgets,
}

/// A decision: allow, deny with a reason, or leave it to Claude Code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Allowed.
    Allow,
    /// Denied, with the reason the agent sees.
    Deny(String),
    /// No opinion.
    Pass,
}

impl Policy {
    /// Builds the policy for `roster`; `source_hash` identifies the `studio/` sources.
    pub fn build(
        roster: &Roster,
        catalog: &ToolCatalog,
        studio: &Path,
        source_hash: String,
    ) -> Self {
        let roles = roster
            .roles()
            .iter()
            .map(|r| {
                let bash = r
                    .harness_tools
                    .contains(&HarnessTool::Bash)
                    .then(|| r.bash_allow.clone());
                let policy = RolePolicy {
                    tools: r.mcp_tools.iter().cloned().collect(),
                    write: r.write.clone(),
                    bash,
                    count: r.count.get(),
                };
                (r.role.to_string(), policy)
            })
            .collect();
        let path_gates = roster
            .team
            .gates
            .iter()
            .filter_map(|g| match &g.on {
                GateOn::Paths(paths) => Some(PathGate {
                    paths: paths.clone(),
                    roles: g
                        .require_any
                        .iter()
                        .filter_map(|p| match p {
                            GateParty::Role(r) => Some(r.to_string()),
                            GateParty::Human => None,
                        })
                        .collect(),
                }),
                GateOn::TaskTransition { .. } => None,
            })
            .collect();
        let budgets = &roster.team.budgets;
        Self {
            studio: studio.to_string_lossy().into_owned(),
            source_hash,
            main_session_role: roster.team.main_session_actor().to_string(),
            roles,
            deny: roster.team.paths.deny.clone(),
            path_gates,
            read_only_tools: catalog.read_only().map(str::to_owned).collect(),
            budgets: TaskBudgets {
                calls: budgets
                    .max_mcp_calls_per_task
                    .map(std::num::NonZeroU32::get),
                mutations: budgets
                    .max_mutations_per_task
                    .map(std::num::NonZeroU32::get),
            },
        }
    }

    /// Reads the policy sync wrote for the project at `root`.
    pub fn load(root: &Path) -> Result<Self, StudioError> {
        let path = root.join(POLICY_PATH);
        let text = std::fs::read_to_string(&path).map_err(|e| StudioError::io(&path, &e))?;
        serde_json::from_str(&text).map_err(|e| StudioError::Invalid {
            file: POLICY_PATH.into(),
            message: e.to_string(),
        })
    }

    /// True if `studio/` changed since the policy was built.
    pub fn is_stale(&self, root: &Path) -> bool {
        source_hash(&root.join(&self.studio)).map_or(true, |hash| hash != self.source_hash)
    }

    /// The policy as written by sync.
    pub fn to_text(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// May `role` (none: an agent outside the team) call engine tool `tool`?
    pub fn tool(&self, role: Option<&str>, tool: &str) -> Decision {
        let (who, allowed): (String, Vec<&str>) =
            match role.and_then(|r| self.roles.get(r).map(|p| (r, p))) {
                Some((name, policy)) => (
                    format!("role {name}"),
                    policy.tools.iter().map(String::as_str).collect(),
                ),
                None => (
                    "an agent outside the Studio team".into(),
                    self.read_only_tools.iter().map(String::as_str).collect(),
                ),
            };
        match allowed.contains(&tool) {
            true => Decision::Allow,
            false => Decision::Deny(format!(
                "{who} may not call {tool}; its engine tools are: {}",
                allowed.join(", ")
            )),
        }
    }

    /// May `role` change the project-relative `path`?
    pub fn write(&self, role: Option<&str>, path: &str) -> Decision {
        if any_match(&self.deny, path) {
            return Decision::Deny(format!(
                "{path} is off limits to every agent ([paths] deny in studio/team.toml)"
            ));
        }
        let gated = self.path_gates.iter().find(|g| any_match(&g.paths, path));
        if gated.is_some_and(|gate| !role.is_some_and(|r| gate.roles.iter().any(|g| g == r))) {
            return Decision::Deny(format!(
                "{path} may be changed only by a human (a gate in studio/team.toml)"
            ));
        }
        let Some((name, policy)) = role.and_then(|r| self.roles.get(r).map(|p| (r, p))) else {
            return Decision::Deny(format!(
                "agents outside the Studio team may not change {path}"
            ));
        };
        match any_match(&policy.write, path) {
            true => Decision::Allow,
            false => Decision::Deny(format!(
                "role {name} may not change {path}; its paths are: {}",
                match policy.write.is_empty() {
                    true => "none".to_owned(),
                    false => policy.write.join(", "),
                }
            )),
        }
    }

    /// May `role` run `command`?
    pub fn bash(&self, role: Option<&str>, command: &str) -> Decision {
        let Some((name, policy)) = role.and_then(|r| self.roles.get(r).map(|p| (r, p))) else {
            return Decision::Deny(
                "agents outside the Studio team may not run shell commands".into(),
            );
        };
        match &policy.bash {
            None => Decision::Deny(format!("role {name} has no shell")),
            Some(patterns) if patterns.is_empty() => Decision::Pass,
            Some(patterns) if patterns.iter().any(|p| command_matches(p, command)) => {
                Decision::Allow
            }
            Some(patterns) => Decision::Deny(format!(
                "role {name} may run only: {}",
                patterns
                    .iter()
                    .map(|p| format!("`{p}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

/// Hash of the Studio sources the policy depends on: `team.toml` and `agents/*.md`, by name.
pub fn source_hash(studio: &Path) -> Result<String, StudioError> {
    let mut files: Vec<PathBuf> = vec![studio.join("team.toml")];
    let agents = studio.join("agents");
    let mut roles: Vec<PathBuf> = std::fs::read_dir(&agents)
        .map_err(|e| StudioError::io(&agents, &e))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    roles.sort();
    files.extend(roles);
    let mut hasher = blake3::Hasher::new();
    for file in files {
        let text = std::fs::read(&file).map_err(|e| StudioError::io(&file, &e))?;
        hasher.update(file.file_name().unwrap_or_default().as_encoded_bytes());
        hasher.update(&(text.len() as u64).to_le_bytes());
        hasher.update(&text);
    }
    Ok(hasher.finalize().to_hex()[..16].to_owned())
}

/// `path` relative to `root`, if it stays inside it (resolving `..` and symlinks of existing parents).
pub fn project_relative(root: &Path, path: &Path) -> Option<String> {
    let root = root.canonicalize().ok()?;
    let joined = match path.is_absolute() {
        true => path.to_owned(),
        false => root.join(path),
    };
    let resolved = resolve(&joined)?;
    let relative = resolved.strip_prefix(&root).ok()?;
    let text = relative.to_string_lossy().replace('\\', "/");
    (!text.is_empty()).then_some(text)
}

/// Canonicalizes the deepest existing ancestor and appends the rest, refusing `..` after it.
fn resolve(path: &Path) -> Option<PathBuf> {
    let mut existing = path.to_owned();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        rest.push(existing.file_name()?.to_owned());
        existing = existing.parent()?.to_owned();
    }
    let mut resolved = existing.canonicalize().ok()?;
    for part in rest.iter().rev() {
        match Path::new(part).components().next() {
            Some(Component::Normal(_)) => resolved.push(part),
            _ => return None,
        }
    }
    Some(resolved)
}

fn any_match(patterns: &[String], path: &str) -> bool {
    patterns
        .iter()
        .filter_map(|p| path_glob(p))
        .any(|g| g.is_match(path))
}

fn path_glob(pattern: &str) -> Option<GlobMatcher> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .ok()
        .map(|g| g.compile_matcher())
}

/// Shell patterns: `*` matches anything, including spaces and slashes.
fn command_matches(pattern: &str, command: &str) -> bool {
    Glob::new(pattern).is_ok_and(|g| g.compile_matcher().is_match(command.trim()))
}
