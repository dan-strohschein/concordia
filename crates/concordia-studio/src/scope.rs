//! Scopes: MCP tool name patterns and project path patterns (shared with enforcement in phase 3.5).

use std::collections::BTreeSet;
use std::path::{Component, Path};

use concordia_api::MethodRegistry;
use concordia_core::suggest::did_you_mean;
use globset::{Glob, GlobMatcher};

use crate::error::StudioError;

/// The MCP tool names a project offers to agents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolCatalog {
    names: BTreeSet<String>,
}

impl ToolCatalog {
    /// A catalog of the given tool names.
    pub fn from_names<S: Into<String>>(names: impl IntoIterator<Item = S>) -> Self {
        Self {
            names: names.into_iter().map(Into::into).collect(),
        }
    }

    /// The agent tools of an engine's methods (what `concordia mcp` lists).
    pub fn from_methods(methods: &MethodRegistry) -> Self {
        Self::from_names(
            methods
                .iter()
                .filter(|m| m.is_agent_tool())
                .map(concordia_api::MethodInfo::tool_name),
        )
    }

    /// Every tool name, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(String::as_str)
    }

    /// The sorted tools `patterns` match; every pattern must match at least one.
    pub fn expand(&self, patterns: &[String]) -> Result<Vec<String>, StudioError> {
        let mut matched = BTreeSet::new();
        let mut unknown = Vec::new();
        for pattern in patterns {
            let glob = matcher(pattern)?;
            let hits: Vec<&String> = self.names.iter().filter(|n| glob.is_match(n)).collect();
            match hits.is_empty() {
                true => unknown.push(pattern.clone()),
                false => matched.extend(hits.into_iter().cloned()),
            }
        }
        match unknown.is_empty() {
            true => Ok(matched.into_iter().collect()),
            false => Err(StudioError::UnknownTools {
                file: String::new(),
                suggestions: self.suggest(&unknown),
                patterns: unknown,
            }),
        }
    }

    fn suggest(&self, unknown: &[String]) -> Vec<String> {
        unknown
            .iter()
            .flat_map(|p| did_you_mean(p.trim_end_matches('*'), self.names()))
            .map(str::to_owned)
            .collect()
    }
}

fn matcher(pattern: &str) -> Result<GlobMatcher, StudioError> {
    Glob::new(pattern)
        .map(|g| g.compile_matcher())
        .map_err(|e| StudioError::invalid(format!("bad pattern {pattern:?}: {e}")))
}

/// Checks that a path pattern stays inside the project (relative, no `..`, no `~`).
pub fn check_project_pattern(pattern: &str) -> Result<(), StudioError> {
    let path = Path::new(pattern);
    let escapes = pattern.is_empty()
        || pattern.starts_with('~')
        || path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)));
    match escapes {
        true => Err(StudioError::PathOutsideProject {
            file: String::new(),
            path: pattern.into(),
        }),
        false => matcher(pattern).map(|_| ()),
    }
}
