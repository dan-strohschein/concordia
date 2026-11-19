//! Engine skills: `studio/skills/<name>/SKILL.md` (Claude Code's skill format).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::claude_code::CLAUDE_SKILL_FIELDS;
use crate::error::StudioError;
use crate::frontmatter;

/// Claude Code truncates `description` plus `when_to_use` beyond this in its skill listing.
const LISTING_LIMIT: usize = 1536;

/// A parsed, validated skill file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFile {
    /// The skill's name (its folder name).
    pub name: String,
    /// What it does; Claude Code lists it.
    pub description: String,
    /// When to use it (listed with the description).
    pub when_to_use: Option<String>,
    /// The Markdown body.
    pub body: String,
}

/// The frontmatter fields Studio reads; the other documented fields pass through to Claude Code.
#[derive(Debug, Deserialize)]
struct Fields {
    name: String,
    description: String,
    when_to_use: Option<String>,
}

impl SkillFile {
    /// Parses `text` as the skill in folder `name`; fields must be documented ones.
    pub fn parse(name: &str, text: &str) -> Result<Self, StudioError> {
        let file = format!("studio/skills/{name}/SKILL.md");
        let (yaml, body) = frontmatter::split(text).map_err(|e| e.in_file(&file))?;
        let fields: Fields =
            frontmatter::parse(yaml, &CLAUDE_SKILL_FIELDS).map_err(|e| e.in_file(&file))?;
        let problem = if fields.name != name {
            Some(format!(
                "`name: {}` must match the folder name {name}",
                fields.name
            ))
        } else if fields.description.trim().is_empty() {
            Some("`description` is required (Claude Code lists it)".to_owned())
        } else {
            let listed = fields.description.chars().count()
                + fields
                    .when_to_use
                    .as_deref()
                    .map_or(0, |w| w.chars().count());
            (listed > LISTING_LIMIT).then(|| {
                format!("description plus when_to_use is {listed} chars; Claude Code truncates beyond {LISTING_LIMIT}")
            })
        };
        if let Some(message) = problem {
            return Err(StudioError::Frontmatter { file, message });
        }
        Ok(Self {
            name: fields.name,
            description: fields.description,
            when_to_use: fields.when_to_use,
            body: body.to_owned(),
        })
    }
}

/// `<dir>/<name>/SKILL.md` texts, sorted by name, unvalidated (missing `dir` = none).
pub(crate) fn read_all(dir: &Path) -> Result<Vec<(String, String)>, StudioError> {
    let mut out = Vec::new();
    for sub in sorted_dirs(dir)? {
        let file = sub.join("SKILL.md");
        if let (Some(name), Ok(text)) = (sub.file_name(), std::fs::read_to_string(&file)) {
            out.push((name.to_string_lossy().into_owned(), text));
        }
    }
    Ok(out)
}

/// Like [`read_all`], with every skill validated by [`SkillFile::parse`].
pub(crate) fn load_all(dir: &Path) -> Result<Vec<(String, String)>, StudioError> {
    let skills = read_all(dir)?;
    for (name, text) in &skills {
        SkillFile::parse(name, text)?;
    }
    Ok(skills)
}

fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>, StudioError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StudioError::io(dir, &e)),
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    paths.sort();
    Ok(paths)
}
