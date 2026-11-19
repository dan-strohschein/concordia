//! The resolved roster: role files with the team's overrides applied and tool patterns expanded.

use std::num::NonZeroU32;
use std::path::Path;

use concordia_core::suggest::did_you_mean;

use crate::error::StudioError;
use crate::gate::GateParty;
use crate::role::{Effort, HarnessTool, RoleFile, RoleId};
use crate::scope::ToolCatalog;
use crate::team::Team;

/// An enabled role, ready for sync and enforcement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRole {
    /// Id.
    pub role: RoleId,
    /// Human-readable name.
    pub display_name: String,
    /// When to delegate to it.
    pub description: String,
    /// Instances.
    pub count: NonZeroU32,
    /// Model hint.
    pub model: String,
    /// Effort hint.
    pub effort: Option<Effort>,
    /// Engine MCP tools, expanded and sorted.
    pub mcp_tools: Vec<String>,
    /// Harness tools.
    pub harness_tools: Vec<HarnessTool>,
    /// Allowed shell command patterns.
    pub bash_allow: Vec<String>,
    /// Writable path patterns.
    pub write: Vec<String>,
    /// Readable path patterns.
    pub read: Vec<String>,
    /// Required reviewers.
    pub review: Vec<GateParty>,
    /// Preloaded skills.
    pub skills: Vec<String>,
    /// Turn limit (the lower of the role's and the team's).
    pub max_turns: Option<NonZeroU32>,
    /// Display color.
    pub color: Option<String>,
    /// The role's instructions.
    pub body: String,
}

/// A role the team turned off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisabledRole {
    /// Id.
    pub role: RoleId,
    /// Why.
    pub reason: String,
}

/// The team with every role resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Roster {
    /// The team file.
    pub team: Team,
    roles: Vec<ResolvedRole>,
    disabled: Vec<DisabledRole>,
}

impl Roster {
    /// Applies `team` to `roles`, expanding tool patterns against `catalog`.
    pub fn resolve(
        team: &Team,
        roles: Vec<RoleFile>,
        catalog: &ToolCatalog,
    ) -> Result<Self, StudioError> {
        check_overrides(team, &roles)?;
        let mut resolved = Vec::new();
        let mut disabled = Vec::new();
        for file in roles {
            let id = file.spec.role.clone();
            match team.roles.get(id.as_str()) {
                Some(o) if !o.enabled => disabled.push(DisabledRole {
                    role: id,
                    reason: o.reason.clone(),
                }),
                _ => resolved.push(resolve_role(team, file, catalog)?),
            }
        }
        resolved.sort_by(|a, b| a.role.cmp(&b.role));
        disabled.sort_by(|a, b| a.role.cmp(&b.role));
        let roster = Self {
            team: team.clone(),
            roles: resolved,
            disabled,
        };
        roster.check_director()?;
        Ok(roster)
    }

    /// Reads `<studio>/team.toml` and `<studio>/agents/*.md`, validates and resolves them.
    pub fn load(studio: &Path, catalog: &ToolCatalog) -> Result<Self, StudioError> {
        let team_path = studio.join("team.toml");
        let team = Team::parse(&read(&team_path)?)?;
        let roles = load_roles(&studio.join("agents"), catalog)?;
        let skills = crate::skill::load_all(&studio.join("skills"))?;
        check_skills(&roles, skills.iter().map(|(name, _)| name.as_str()))?;
        Self::resolve(&team, roles, catalog)
    }

    /// An enabled role.
    pub fn role(&self, id: &str) -> Option<&ResolvedRole> {
        self.roles.iter().find(|r| r.role.as_str() == id)
    }

    /// Enabled roles, sorted by id.
    pub fn roles(&self) -> &[ResolvedRole] {
        &self.roles
    }

    /// Disabled roles, sorted by id.
    pub fn disabled(&self) -> &[DisabledRole] {
        &self.disabled
    }

    /// The director's role id.
    pub fn director(&self) -> &RoleId {
        &self.team.team.director
    }

    fn check_director(&self) -> Result<(), StudioError> {
        let director = self.director();
        let message = match (
            self.role(director.as_str()),
            self.disabled.iter().any(|d| &d.role == director),
        ) {
            (Some(_), _) => return Ok(()),
            (None, true) => {
                format!("the director role {director:?} is disabled; the main session needs it")
            }
            (None, false) => {
                format!("the director role {director:?} has no studio/agents/{director}.md")
            }
        };
        Err(StudioError::Director { message })
    }
}

fn check_overrides(team: &Team, roles: &[RoleFile]) -> Result<(), StudioError> {
    let ids: Vec<&str> = roles.iter().map(|r| r.spec.role.as_str()).collect();
    match team.roles.keys().find(|k| !ids.contains(&k.as_str())) {
        None => Ok(()),
        Some(unknown) => Err(StudioError::UnknownKey {
            file: "studio/team.toml [roles]".into(),
            key: unknown.clone(),
            did_you_mean: did_you_mean(unknown, ids.iter().copied())
                .first()
                .map(|s| (*s).to_owned()),
        }),
    }
}

fn resolve_role(
    team: &Team,
    file: RoleFile,
    catalog: &ToolCatalog,
) -> Result<ResolvedRole, StudioError> {
    let spec = file.spec;
    let name = format!("{}.md", spec.role);
    let count = team
        .roles
        .get(spec.role.as_str())
        .and_then(|o| o.count)
        .unwrap_or(spec.count);
    let max_turns = match (spec.max_turns, team.budgets.max_turns_per_subagent) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    Ok(ResolvedRole {
        mcp_tools: catalog
            .expand(&spec.mcp_tools)
            .map_err(|e| e.in_file(&name))?,
        role: spec.role,
        display_name: spec.display_name,
        description: spec.description,
        count,
        model: spec.model,
        effort: spec.effort,
        harness_tools: spec.harness_tools,
        bash_allow: spec.bash_allow,
        write: spec.paths.write,
        read: spec.paths.read,
        review: spec.review.requires,
        skills: spec.skills,
        max_turns,
        color: spec.color,
        body: file.body,
    })
}

/// Parses and validates every `*.md` in `dir`, sorted by file name.
/// Every skill a role preloads must exist in `studio/skills/`.
fn check_skills<'a>(
    roles: &[RoleFile],
    skills: impl Iterator<Item = &'a str> + Clone,
) -> Result<(), StudioError> {
    for role in roles {
        for skill in &role.spec.skills {
            if skills.clone().any(|s| s == skill) {
                continue;
            }
            let hint = did_you_mean(skill, skills.clone())
                .first()
                .map(|s| format!("; did you mean {s}?"))
                .unwrap_or_default();
            return Err(StudioError::invalid(format!(
                "studio/agents/{}.md preloads skill {skill}, but studio/skills/{skill}/SKILL.md does not exist{hint}",
                role.spec.role
            )));
        }
    }
    Ok(())
}

fn load_roles(dir: &Path, catalog: &ToolCatalog) -> Result<Vec<RoleFile>, StudioError> {
    let entries = std::fs::read_dir(dir).map_err(|e| StudioError::io(dir, &e))?;
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let name = format!("studio/agents/{stem}.md");
            let role = RoleFile::parse(&read(path)?).map_err(|e| e.in_file(&name))?;
            role.validate(&stem, catalog)?;
            Ok(role)
        })
        .collect()
}

fn read(path: &Path) -> Result<String, StudioError> {
    std::fs::read_to_string(path).map_err(|e| StudioError::io(path, &e))
}
