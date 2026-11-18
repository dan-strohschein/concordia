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

/// Gate test-binary prefix for a milestone: `M-1` → `m_minus1`, `M3` → `m3`.
pub fn gate_prefix(milestone: &str) -> Result<String, String> {
    let number = milestone
        .strip_prefix('M')
        .filter(|n| !n.is_empty())
        .ok_or_else(|| format!("milestone must look like M<n>, got {milestone:?}"))?;
    let digits = number.strip_prefix('-').unwrap_or(number);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "milestone number must be an integer, got {milestone:?}"
        ));
    }
    Ok(format!("m{}", number.replace('-', "_minus")))
}

/// Path to a built WASM fixture, or an error naming the command that builds it.
pub fn wasm_fixture_path(name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || name.contains(['/', '.']) {
        return Err(format!("invalid fixture name {name:?}"));
    }
    let dir = std::env::var_os("CONCORDIA_WASM_FIXTURES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("target/wasm-fixtures/wasm32-wasip2/release"));
    let path = dir.join(format!("{name}.wasm"));
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "fixture missing: {} (run `just wasm`)",
            path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::gate_prefix;

    #[test]
    fn gate_prefix_of_m_minus1_is_m_minus1() {
        assert_eq!(gate_prefix("M-1").unwrap(), "m_minus1");
    }

    #[test]
    fn gate_prefix_of_m3_is_m3() {
        assert_eq!(gate_prefix("M3").unwrap(), "m3");
    }

    #[test]
    fn gate_prefix_rejects_malformed_milestones() {
        for bad in ["", "M", "3", "Mx", "M-"] {
            assert!(gate_prefix(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
