//! Fixtures for `studio sync`: a project with the default roster and one skill.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use concordia_studio::{Roster, SyncOptions, SyncReport, sync};

pub const SKILL: &str = "---\nname: concordia-demo\ndescription: A demo skill used by the sync tests.\n---\n# Demo\nSteps.\n";

/// A temp project with the default Studio files and a `concordia-demo` skill.
pub fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    concordia_studio::init(dir.path(), Path::new("studio")).unwrap();
    let skill = dir.path().join("studio/skills/concordia-demo");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), SKILL).unwrap();
    dir
}

/// The project's roster, resolved against the `concordia new` tool set.
pub fn roster(root: &Path) -> Roster {
    Roster::load(&root.join("studio"), &super::catalog()).unwrap()
}

/// Syncs with the given options.
pub fn run(root: &Path, options: SyncOptions) -> SyncReport {
    sync(root, Path::new("studio"), &roster(root), options).unwrap()
}

/// Syncs and writes.
pub fn write(root: &Path) -> SyncReport {
    run(root, SyncOptions::default())
}

pub fn read(root: &Path, relative: &str) -> String {
    std::fs::read_to_string(root.join(relative)).unwrap_or_else(|e| panic!("{relative}: {e}"))
}

/// Generated agent files, sorted.
pub fn agent_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join(".claude/agents"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    files
}

/// The `tools:` line of an agent file, split.
pub fn tools(text: &str) -> Vec<String> {
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("tools: "))
        .unwrap_or_else(|| panic!("no tools line:\n{text}"));
    // Split on ", " outside parentheses: `Agent(a, b)` is one tool.
    let (mut out, mut current, mut depth) = (Vec::new(), String::new(), 0);
    for c in line.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(current.trim().to_owned());
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    out.push(current.trim().to_owned());
    out
}

/// The frontmatter keys of an agent file (comments skipped).
pub fn frontmatter_keys(text: &str) -> Vec<String> {
    let yaml = text
        .strip_prefix("---\n")
        .unwrap()
        .split("\n---\n")
        .next()
        .unwrap();
    yaml.lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with(' ') && !l.starts_with('-'))
        .filter_map(|l| l.split_once(':').map(|(k, _)| k.to_owned()))
        .collect()
}
