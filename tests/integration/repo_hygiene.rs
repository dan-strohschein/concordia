//! Repository conventions, checked on every `just test`.

use concordia_tests::{read_repo_file, repo_root, toml_at, tracked_files};
use regex::Regex;

fn toolchain_channel() -> String {
    let doc = toml_at("rust-toolchain.toml");
    doc["toolchain"]["channel"]
        .as_str()
        .expect("channel is a string")
        .to_owned()
}

fn workspace_package() -> toml::Table {
    let doc = toml_at("Cargo.toml");
    doc["workspace"]["package"]
        .as_table()
        .expect("[workspace.package]")
        .clone()
}

fn member_manifests() -> Vec<std::path::PathBuf> {
    let crates = std::fs::read_dir(repo_root().join("crates")).expect("crates/ exists");
    let mut manifests: Vec<_> = crates
        .filter_map(Result::ok)
        .map(|e| e.path().join("Cargo.toml"))
        .filter(|p| p.is_file())
        .collect();
    manifests.push(repo_root().join("tests/Cargo.toml"));
    manifests
}

fn gitignore_lines() -> Vec<String> {
    read_repo_file(".gitignore")
        .lines()
        .map(str::trim)
        .map(str::to_owned)
        .collect()
}

#[test]
fn rust_toolchain_pins_exact_stable_version() {
    let channel = toolchain_channel();
    assert!(
        Regex::new(r"^1\.\d+\.\d+$").unwrap().is_match(&channel),
        "channel {channel}"
    );
    let doc = toml_at("rust-toolchain.toml");
    let list = |key: &str| doc["toolchain"][key].as_array().expect(key).clone();
    assert!(
        list("components")
            .iter()
            .any(|c| c.as_str() == Some("rustfmt"))
    );
    assert!(
        list("components")
            .iter()
            .any(|c| c.as_str() == Some("clippy"))
    );
    assert!(
        list("targets")
            .iter()
            .any(|t| t.as_str() == Some("wasm32-wasip2"))
    );
}

#[test]
fn workspace_rust_version_matches_toolchain_pin() {
    let rust_version = workspace_package()["rust-version"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        toolchain_channel().starts_with(&rust_version),
        "{rust_version} vs pin"
    );
}

#[test]
fn workspace_uses_edition_2024_and_resolver_3() {
    let doc = toml_at("Cargo.toml");
    assert_eq!(doc["workspace"]["resolver"].as_str(), Some("3"));
    assert_eq!(workspace_package()["edition"].as_str(), Some("2024"));
}

#[test]
fn every_crate_inherits_workspace_package_fields() {
    for path in member_manifests() {
        let doc: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let package = doc["package"].as_table().unwrap();
        for field in ["edition", "license", "rust-version"] {
            assert_eq!(
                package[field]["workspace"].as_bool(),
                Some(true),
                "{field} in {}",
                path.display()
            );
        }
        assert_eq!(
            doc["lints"]["workspace"].as_bool(),
            Some(true),
            "{}",
            path.display()
        );
    }
}

#[test]
fn every_crate_name_uses_concordia_prefix() {
    let crates = std::fs::read_dir(repo_root().join("crates")).unwrap();
    for entry in crates.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            name.starts_with("concordia-") || name.ends_with("-sys"),
            "bad crate name {name}"
        );
    }
}

#[test]
fn gitignore_excludes_build_outputs() {
    let lines = gitignore_lines();
    for required in [
        "/target",
        ".concordia/",
        "node_modules/",
        ".claude/settings.local.json",
        ".DS_Store",
    ] {
        assert!(lines.iter().any(|l| l == required), "missing {required}");
    }
}

#[test]
fn cargo_lock_is_committed_not_ignored() {
    assert!(repo_root().join("Cargo.lock").is_file());
    assert!(!gitignore_lines().iter().any(|l| l.contains("Cargo.lock")));
}

#[test]
fn private_dir_is_gitignored() {
    assert!(gitignore_lines().iter().any(|l| l == "/private/"));
}

#[test]
fn no_home_paths_in_tracked_files() {
    let home_path = Regex::new(r"/Users/[A-Za-z0-9_]|/home/[a-z]").unwrap();
    for file in tracked_files() {
        let Ok(text) = std::fs::read_to_string(repo_root().join(&file)) else {
            continue;
        };
        assert!(!home_path.is_match(&text), "home path in {file}");
    }
}

#[test]
fn no_private_markers_in_tracked_files() {
    let Ok(markers) = std::fs::read_to_string(repo_root().join("private/markers.txt")) else {
        eprintln!("note: private/markers.txt absent; marker check skipped");
        return;
    };
    let markers: Vec<&str> = markers
        .lines()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .collect();
    for file in tracked_files() {
        let Ok(text) = std::fs::read_to_string(repo_root().join(&file)) else {
            continue;
        };
        let hit = markers.iter().find(|m| text.contains(**m));
        assert!(hit.is_none(), "private marker in {file}");
    }
}

fn assert_contains_all(relative: &str, needles: &[&str]) {
    let text = read_repo_file(relative).to_lowercase();
    for needle in needles {
        assert!(
            text.contains(&needle.to_lowercase()),
            "{relative} lacks {needle:?}"
        );
    }
}

fn crate_dirs() -> Vec<std::path::PathBuf> {
    let crates = std::fs::read_dir(repo_root().join("crates")).unwrap();
    crates
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

const CRATE_AGENTS_SECTIONS: [&str; 5] = [
    "## Purpose",
    "## Invariants",
    "## How to test",
    "## References",
    "## Status",
];

const ENGINEERING_RULES: [&str; 6] = [
    "## Engineering rules",
    "tests first",
    "SOLID",
    "search for existing functionality",
    "single responsibility",
    "validate",
];

#[test]
fn root_has_dual_license_files() {
    assert_contains_all(
        "LICENSE-MIT",
        &[
            "Permission is hereby granted, free of charge",
            "The Concordia contributors",
        ],
    );
    assert_contains_all(
        "LICENSE-APACHE",
        &["Apache License", "Version 2.0, January 2004"],
    );
}

#[test]
fn contributing_documents_dco_and_clean_room() {
    assert_contains_all(
        "CONTRIBUTING.md",
        &[
            "Developer Certificate of Origin",
            "Signed-off-by",
            "docs/clean-room.md",
            "tests first",
        ],
    );
}

#[test]
fn root_agents_md_has_required_sections() {
    assert_contains_all(
        "AGENTS.md",
        &[
            "## Rules",
            "## Commands",
            "## Layout",
            "## Where to start",
            "just test",
            "just lint",
            "just gate",
        ],
    );
}

#[test]
fn directory_skeleton_exists() {
    let tracked = tracked_files();
    let dirs = [
        "crates/",
        "wit/",
        "editor/",
        "studio/",
        "tests/gates/",
        "tests/golden/",
        "tests/replays/",
        "tests/fixtures/",
        "tools/",
        "third_party/",
        "examples/",
        "docs/adr/",
    ];
    for dir in dirs {
        assert!(
            tracked.iter().any(|f| f.starts_with(dir)),
            "{dir} has no tracked file"
        );
    }
}

#[test]
fn every_crate_has_agents_md_and_readme() {
    for dir in crate_dirs() {
        for entry in ["AGENTS.md", "README.md", "src", "tests"] {
            assert!(dir.join(entry).exists(), "{} lacks {entry}", dir.display());
        }
    }
}

#[test]
fn every_crate_agents_md_has_required_sections() {
    for dir in crate_dirs() {
        let relative = dir.strip_prefix(repo_root()).unwrap().join("AGENTS.md");
        assert_contains_all(relative.to_str().unwrap(), &CRATE_AGENTS_SECTIONS);
    }
}

#[test]
fn crate_template_matches_required_sections() {
    assert_contains_all("tools/templates/crate/AGENTS.md", &CRATE_AGENTS_SECTIONS);
}

#[test]
fn claude_md_and_agents_md_contain_engineering_rules() {
    for file in ["CLAUDE.md", "AGENTS.md", "tools/templates/crate/AGENTS.md"] {
        assert_contains_all(file, &ENGINEERING_RULES);
    }
}

#[test]
fn adr_files_follow_numbering() {
    let pattern = Regex::new(r"^(\d{4})-[a-z0-9-]+\.md$").unwrap();
    let mut numbers = std::collections::HashSet::new();
    for entry in std::fs::read_dir(repo_root().join("docs/adr"))
        .unwrap()
        .filter_map(Result::ok)
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let caps = pattern
            .captures(&name)
            .unwrap_or_else(|| panic!("bad ADR name {name}"));
        assert!(
            numbers.insert(caps[1].to_owned()),
            "duplicate ADR number in {name}"
        );
    }
}
