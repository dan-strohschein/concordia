//! Role files: `studio/agents/<role>.md`, YAML frontmatter plus the role's instructions.

use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::StudioError;
use crate::frontmatter;
use crate::gate::GateParty;
use crate::scope::{ToolCatalog, check_project_pattern};

/// Longest role id (an actor name is at most 64 characters, including `#n`).
const MAX_ROLE: usize = 48;

/// Model hints Claude Code understands besides a full model id.
const MODEL_ALIASES: [&str; 5] = ["inherit", "sonnet", "opus", "haiku", "fable"];

/// A role id: kebab-case (`level-designer`), at most 48 characters; also the actor name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RoleId(String);

impl RoleId {
    /// Validates a role id.
    pub fn new(text: &str) -> Result<Self, StudioError> {
        let kebab = text.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        });
        let starts = text.bytes().next().is_some_and(|b| b.is_ascii_lowercase());
        match kebab && starts && text.len() <= MAX_ROLE {
            true => Ok(Self(text.to_owned())),
            false => Err(StudioError::invalid(format!(
                "role id {text:?} must be kebab-case (e.g. level-designer), start with a letter and have at most {MAX_ROLE} characters"
            ))),
        }
    }

    /// The id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RoleId {
    type Err = StudioError;
    fn from_str(s: &str) -> Result<Self, StudioError> {
        Self::new(s)
    }
}

impl TryFrom<String> for RoleId {
    type Error = StudioError;
    fn try_from(s: String) -> Result<Self, StudioError> {
        Self::new(&s)
    }
}

impl From<RoleId> for String {
    fn from(id: RoleId) -> Self {
        id.0
    }
}

/// Reasoning effort hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
    /// Extra high.
    Xhigh,
    /// Maximum.
    Max,
}

/// Harness-neutral tool names, mapped per compile target (e.g. Claude Code `Read, Grep, Glob`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HarnessTool {
    /// Read files.
    Read,
    /// Search files.
    Search,
    /// Edit files.
    Edit,
    /// Create files.
    Write,
    /// Run shell commands.
    Bash,
}

/// A role's project path patterns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RolePaths {
    /// Paths the role may write (gitignore-style globs).
    #[serde(default)]
    pub write: Vec<String>,
    /// Paths the role may read.
    #[serde(default = "everything")]
    pub read: Vec<String>,
}

impl Default for RolePaths {
    fn default() -> Self {
        Self {
            write: Vec::new(),
            read: everything(),
        }
    }
}

fn everything() -> Vec<String> {
    vec!["**".into()]
}

/// Who must review the role's work.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    /// Parties that may move the role's tasks to done.
    #[serde(default)]
    pub requires: Vec<GateParty>,
}

impl Review {
    fn is_empty(&self) -> bool {
        self.requires.is_empty()
    }
}

/// A role file's frontmatter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSpec {
    /// Id; equals the file stem.
    pub role: RoleId,
    /// Human-readable name.
    pub display_name: String,
    /// When the Director should delegate to this role.
    pub description: String,
    /// Model hint: inherit, sonnet, opus, haiku, fable or a full model id.
    #[serde(default = "inherit")]
    pub model: String,
    /// Effort hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Default number of instances.
    #[serde(default = "one")]
    pub count: NonZeroU32,
    /// Harness tools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub harness_tools: Vec<HarnessTool>,
    /// Allowed shell command patterns (when `bash` is a harness tool).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bash_allow: Vec<String>,
    /// Engine MCP tool name patterns, e.g. `scene_*`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_tools: Vec<String>,
    /// Path scopes.
    #[serde(default)]
    pub paths: RolePaths,
    /// Required reviewers.
    #[serde(default, skip_serializing_if = "Review::is_empty")]
    pub review: Review,
    /// Skills preloaded for the role.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// Turn limit per run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<NonZeroU32>,
    /// Display color (matches the generated agent's `color`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// The frontmatter keys a role file may use.
const ROLE_KEYS: [&str; 14] = [
    "role",
    "display_name",
    "description",
    "model",
    "effort",
    "count",
    "harness_tools",
    "bash_allow",
    "mcp_tools",
    "paths",
    "review",
    "skills",
    "max_turns",
    "color",
];

fn inherit() -> String {
    "inherit".into()
}

fn one() -> NonZeroU32 {
    NonZeroU32::MIN
}

impl RoleSpec {
    /// A spec with defaults for everything but the id, name and description.
    pub fn new(
        role: RoleId,
        display_name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            role,
            display_name: display_name.into(),
            description: description.into(),
            model: inherit(),
            effort: None,
            count: one(),
            harness_tools: Vec::new(),
            bash_allow: Vec::new(),
            mcp_tools: Vec::new(),
            paths: RolePaths::default(),
            review: Review::default(),
            skills: Vec::new(),
            max_turns: None,
            color: None,
        }
    }
}

/// A parsed role file: frontmatter plus the Markdown body (kept verbatim).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleFile {
    /// Frontmatter.
    pub spec: RoleSpec,
    /// Instructions (Markdown).
    pub body: String,
}

impl RoleFile {
    /// A role file from its parts.
    pub fn new(spec: RoleSpec, body: impl Into<String>) -> Self {
        Self {
            spec,
            body: body.into(),
        }
    }

    /// Parses a role file; unknown keys are errors (unlike Claude Code's lenient loader).
    pub fn parse(text: &str) -> Result<Self, StudioError> {
        let (yaml, body) = frontmatter::split(text)?;
        let spec = frontmatter::parse(yaml, &ROLE_KEYS)?;
        Ok(Self::new(spec, body))
    }

    /// The canonical text.
    pub fn to_text(&self) -> String {
        frontmatter::join(&self.spec, &self.body)
    }

    /// Checks the file stem, model hint, tool patterns and path scopes.
    pub fn validate(&self, stem: &str, catalog: &ToolCatalog) -> Result<(), StudioError> {
        let file = format!("{stem}.md");
        if self.spec.role.as_str() != stem {
            return Err(StudioError::RoleStem {
                role: self.spec.role.to_string(),
                stem: stem.into(),
            });
        }
        check_model(&self.spec.model).map_err(|e| e.in_file(&file))?;
        catalog
            .expand(&self.spec.mcp_tools)
            .map_err(|e| e.in_file(&file))?;
        self.spec
            .paths
            .write
            .iter()
            .chain(&self.spec.paths.read)
            .try_for_each(|p| check_project_pattern(p))
            .map_err(|e| e.in_file(&file))
    }
}

fn check_model(model: &str) -> Result<(), StudioError> {
    match MODEL_ALIASES.contains(&model) || model.starts_with("claude-") {
        true => Ok(()),
        false => Err(StudioError::invalid(format!(
            "model {model:?} must be one of {} or a full claude-* model id",
            MODEL_ALIASES.join(", ")
        ))),
    }
}
