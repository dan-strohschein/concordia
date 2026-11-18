//! Shared helpers for Concordia's cross-crate and gate tests.

use std::path::PathBuf;
use std::process::Command;

/// Repository root (the parent of this package).
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tests/ has a parent")
        .canonicalize()
        .expect("repo root exists")
}

/// Reads a file relative to the repository root.
pub fn read_repo_file(relative: &str) -> String {
    assert!(!relative.is_empty(), "relative path must not be empty");
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Parses a TOML file relative to the repository root.
pub fn toml_at(relative: &str) -> toml::Table {
    let text = read_repo_file(relative);
    toml::from_str(&text).unwrap_or_else(|e| panic!("parse {relative}: {e}"))
}

/// Files git would commit: tracked plus untracked-but-not-ignored.
pub fn tracked_files() -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(repo_root())
        .output()
        .expect("git is installed");
    assert!(output.status.success(), "git ls-files failed");
    String::from_utf8(output.stdout)
        .expect("utf-8 paths")
        .lines()
        .map(str::to_owned)
        .collect()
}
