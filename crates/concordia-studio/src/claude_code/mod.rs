//! `studio sync`: compile the roster to Claude Code's native project files (M3 phase 3.4).
//!
//! Agent and skill files are owned outright (marked and hash-checked, so hand edits are refused);
//! `settings.json`, `.mcp.json` and `CLAUDE.md` are merged, touching only Studio's entries.

mod agents;
mod claude_md;
mod json;
mod owned;
mod settings;

use std::path::{Path, PathBuf};

use json::Json;
use owned::{Ownership, mark, ownership};

use crate::error::StudioError;
use crate::roster::Roster;
use crate::scope::ToolCatalog;

/// Subagent frontmatter fields documented by Claude Code 2.1.328; generated files use only these.
pub const CLAUDE_AGENT_FIELDS: [&str; 19] = [
    "name",
    "description",
    "tools",
    "disallowedTools",
    "model",
    "permissionMode",
    "maxTurns",
    "autoCompactWindow",
    "skills",
    "mcpServers",
    "hooks",
    "memory",
    "background",
    "omitClaudeMd",
    "effort",
    "isolation",
    "color",
    "initialPrompt",
    "experimental",
];

/// How to sync.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncOptions {
    /// Report what would change; write nothing.
    pub check: bool,
    /// Overwrite generated files that were edited by hand.
    pub force: bool,
}

/// What sync does to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncAction {
    /// New file.
    Create,
    /// Changed file.
    Update,
    /// Removed (a generated file whose role or skill is gone).
    Delete,
    /// Already up to date.
    Unchanged,
}

/// One file's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Project-relative path.
    pub path: String,
    /// What happens to it.
    pub action: SyncAction,
    /// Unified diff for updates.
    pub diff: Option<String>,
}

/// The result of a sync.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Every file sync owns or merges into.
    pub changes: Vec<FileChange>,
    /// Generated files edited by hand and left alone (sync with `force` to overwrite).
    pub refused: Vec<String>,
}

impl SyncReport {
    /// True if any file is not up to date.
    pub fn is_stale(&self) -> bool {
        !self.refused.is_empty()
            || self
                .changes
                .iter()
                .any(|c| c.action != SyncAction::Unchanged)
    }
}

/// A file sync wants: its content, and whether sync owns it outright (vs merging into it).
struct Desired {
    path: String,
    content: Option<String>,
    owned: bool,
}

/// Generates the Claude Code files for `roster` under `root`.
pub fn sync(
    root: &Path,
    studio: &Path,
    roster: &Roster,
    catalog: &ToolCatalog,
    options: SyncOptions,
) -> Result<SyncReport, StudioError> {
    let mut desired = owned_files(root, studio, roster)?;
    desired.extend(merged_files(root, roster)?);
    let hash = crate::policy::source_hash(&root.join(studio))?;
    desired.push(Desired {
        path: crate::policy::POLICY_PATH.into(),
        content: Some(crate::policy::Policy::build(roster, catalog, studio, hash).to_text()),
        owned: false,
    });
    desired.extend(stale_owned(root, &desired)?);
    desired.sort_by(|a, b| a.path.cmp(&b.path));
    let mut report = SyncReport::default();
    for file in desired {
        plan_one(root, file, options, &mut report)?;
    }
    Ok(report)
}

/// Agent files for enabled roles and skill files from `<studio>/skills`.
fn owned_files(root: &Path, studio: &Path, roster: &Roster) -> Result<Vec<Desired>, StudioError> {
    let mut files: Vec<Desired> = roster
        .roles()
        .iter()
        .map(|role| Desired {
            path: format!(".claude/agents/{}.md", role.role),
            content: Some(agents::agent_file(role, roster)),
            owned: true,
        })
        .collect();
    for (name, text) in skills(&root.join(studio).join("skills"))? {
        let source = format!("{}/skills/{name}/SKILL.md", studio.display());
        files.push(Desired {
            path: format!(".claude/skills/{name}/SKILL.md"),
            content: Some(mark(&text, &source)),
            owned: true,
        });
    }
    Ok(files)
}

fn merged_files(root: &Path, roster: &Roster) -> Result<Vec<Desired>, StudioError> {
    let settings = Json::parse(
        &read_or_empty(&root.join(".claude/settings.json"))?,
        ".claude/settings.json",
    )?;
    let mcp = Json::parse(&read_or_empty(&root.join(".mcp.json"))?, ".mcp.json")?;
    let claude = read_or_empty(&root.join("CLAUDE.md"))?;
    let merged = |path: &str, content: String| Desired {
        path: path.into(),
        content: Some(content),
        owned: false,
    };
    Ok(vec![
        merged(
            ".claude/settings.json",
            settings::merge_settings(settings, roster).to_text(),
        ),
        merged(".mcp.json", settings::merge_mcp(mcp).to_text()),
        merged("CLAUDE.md", claude_md::merge(&claude, roster)),
    ])
}

/// Generated agent and skill files no role or skill wants any more (to delete).
fn stale_owned(root: &Path, desired: &[Desired]) -> Result<Vec<Desired>, StudioError> {
    let wanted: Vec<&str> = desired.iter().map(|d| d.path.as_str()).collect();
    let mut candidates = markdown_in(&root.join(".claude/agents"), ".claude/agents")?;
    for (name, _) in skills(&root.join(".claude/skills"))? {
        candidates.push(format!(".claude/skills/{name}/SKILL.md"));
    }
    Ok(candidates
        .into_iter()
        .filter(|path| !wanted.contains(&path.as_str()))
        .filter(|path| {
            let text = std::fs::read_to_string(root.join(path)).unwrap_or_default();
            ownership(&text) != Ownership::Foreign
        })
        .map(|path| Desired {
            path,
            content: None,
            owned: true,
        })
        .collect())
}

/// Decides one file's action, writing it unless checking; hand-edited owned files are refused.
fn plan_one(
    root: &Path,
    file: Desired,
    options: SyncOptions,
    report: &mut SyncReport,
) -> Result<(), StudioError> {
    let path = root.join(&file.path);
    let current = std::fs::read_to_string(&path).ok();
    let action = match (&current, &file.content) {
        (None, None) => return Ok(()),
        (None, Some(_)) => SyncAction::Create,
        (Some(_), None) => SyncAction::Delete,
        (Some(now), Some(want)) if now == want => SyncAction::Unchanged,
        (Some(_), Some(_)) => SyncAction::Update,
    };
    let guarded = file.owned && action != SyncAction::Create && action != SyncAction::Unchanged;
    if guarded && !options.force && current.as_deref().map(ownership) != Some(Ownership::Owned) {
        report.refused.push(file.path);
        return Ok(());
    }
    let diff = match (action, &current, &file.content) {
        (SyncAction::Update, Some(old), Some(new)) => Some(unified_diff(&file.path, old, new)),
        _ => None,
    };
    if !options.check {
        apply(&path, action, file.content.as_deref())?;
    }
    report.changes.push(FileChange {
        path: file.path,
        action,
        diff,
    });
    Ok(())
}

fn apply(path: &Path, action: SyncAction, content: Option<&str>) -> Result<(), StudioError> {
    match (action, content) {
        (SyncAction::Create | SyncAction::Update, Some(text)) => write_atomic(path, text),
        (SyncAction::Delete, _) => {
            std::fs::remove_file(path).map_err(|e| StudioError::io(path, &e))?;
            if let Some(dir) = path.parent() {
                let _ = std::fs::remove_dir(dir); // only succeeds when empty
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn unified_diff(path: &str, old: &str, new: &str) -> String {
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

/// `<dir>/<name>/SKILL.md` texts, sorted by name.
fn skills(dir: &Path) -> Result<Vec<(String, String)>, StudioError> {
    let mut out = Vec::new();
    for sub in sorted_entries(dir)? {
        let file = sub.join("SKILL.md");
        if let (Some(name), Ok(text)) = (sub.file_name(), std::fs::read_to_string(&file)) {
            if !text.starts_with("---\n") {
                return Err(StudioError::Frontmatter {
                    file: file.display().to_string(),
                    message: "a skill must start with YAML frontmatter (`---`)".into(),
                });
            }
            out.push((name.to_string_lossy().into_owned(), text));
        }
    }
    Ok(out)
}

fn markdown_in(dir: &Path, relative: &str) -> Result<Vec<String>, StudioError> {
    Ok(sorted_entries(dir)?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .filter_map(|p| {
            p.file_name()
                .map(|n| format!("{relative}/{}", n.to_string_lossy()))
        })
        .collect())
}

fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>, StudioError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StudioError::io(dir, &e)),
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    Ok(paths)
}

fn read_or_empty(path: &Path) -> Result<String, StudioError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(StudioError::io(path, &e)),
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), StudioError> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| StudioError::io(dir, &e))?;
    let temp = path.with_extension("sync.tmp");
    std::fs::write(&temp, text).map_err(|e| StudioError::io(&temp, &e))?;
    std::fs::rename(&temp, path).map_err(|e| StudioError::io(path, &e))
}
