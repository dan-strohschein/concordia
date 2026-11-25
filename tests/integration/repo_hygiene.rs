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

/// Golden tolerances live in `tests/golden/<suite>/tolerances.ron` (M4 4.1a, D2); only the
/// testkit's default is in code.
#[test]
fn golden_tolerances_live_in_text() {
    let literal = Regex::new(r"Tolerance\s*\{|Tolerance::mean\(").unwrap();
    for file in tracked_files() {
        if !file.ends_with(".rs") || file.starts_with("crates/concordia-testkit/") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(repo_root().join(&file)) else {
            continue;
        };
        assert!(
            !literal.is_match(&text),
            "{file} sets a golden tolerance in code; put it in tests/golden/<suite>/tolerances.ron"
        );
    }
}

/// Crates that must never pull in the GPU renderer (M4 4.1b, ADR-0101): the shared render types
/// stay GPU-free, and the path tracer stays independent of the renderer it checks.
#[test]
fn gpu_free_crates_keep_their_dependencies() {
    let rules: [(&str, &[&str]); 2] = [
        ("concordia-pathtrace", &["concordia-render"]),
        (
            "concordia-render-types",
            &[
                "concordia-render",
                "concordia-rhi",
                "concordia-rhi-metal",
                "concordia-shaders",
            ],
        ),
    ];
    for (krate, forbidden) in rules {
        let manifest = toml_at(&format!("crates/{krate}/Cargo.toml"));
        let deps: Vec<&String> = ["dependencies", "dev-dependencies", "build-dependencies"]
            .iter()
            .filter_map(|k| manifest.get(*k).and_then(|t| t.as_table()))
            .flat_map(|t| t.keys())
            .collect();
        for dep in deps {
            assert!(
                !forbidden.contains(&dep.as_str()),
                "{krate} must not depend on {dep}"
            );
        }
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

const CRATE_AGENTS_SECTIONS: [&str; 8] = [
    "## Purpose",
    "## Public entry points",
    "## Invariants",
    "## How to test",
    "## Common tasks",
    "## Don'ts",
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

/// M3 refinement G13: the default Studio roster has one home, `crates/concordia-studio/roster/`.
#[test]
fn the_default_roster_has_no_second_home_at_the_repo_root() {
    let stray: Vec<String> = tracked_files()
        .into_iter()
        .filter(|f| f.starts_with("studio/"))
        .collect();
    assert!(stray.is_empty(), "{stray:?}");
}

#[test]
fn directory_skeleton_exists() {
    let tracked = tracked_files();
    let dirs = [
        "crates/",
        "wit/",
        "editor/",
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

fn justfile_recipes() -> std::collections::HashSet<String> {
    let recipe = Regex::new(r"(?m)^@?([a-z][a-z0-9-]*)\b[^:\n=]*:([^=]|$)").unwrap();
    let text = read_repo_file("justfile");
    recipe
        .captures_iter(&text)
        .map(|c| c[1].to_owned())
        .collect()
}

#[test]
fn justfile_defines_required_recipes() {
    let recipes = justfile_recipes();
    let required = [
        "doctor",
        "fmt",
        "fmt-check",
        "lint",
        "test",
        "wasm",
        "deny",
        "gate",
        "gate-tests",
        "gate-live",
        "perf-smoke",
        "bench",
        "bless",
        "ci",
        "new-crate",
    ];
    for name in required {
        assert!(recipes.contains(name), "justfile lacks recipe {name}");
    }
}

#[test]
fn gate_recipe_uses_same_mapping_as_gate_prefix() {
    let text = read_repo_file("justfile");
    assert!(
        text.contains("tr '[:upper:]' '[:lower:]'"),
        "gate recipe must lowercase"
    );
    assert!(
        text.contains("sed 's/-/_minus/'"),
        "gate recipe must map - to _minus"
    );
}

fn string_list(table: &toml::Table, section: &str, key: &str) -> Vec<String> {
    table[section][key]
        .as_array()
        .unwrap_or_else(|| panic!("{section}.{key} is a list"))
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn deny_toml_allows_only_permissive_licenses() {
    let allow = string_list(&toml_at("deny.toml"), "licenses", "allow");
    for required in [
        "MIT",
        "Apache-2.0",
        "Apache-2.0 WITH LLVM-exception",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "ISC",
        "Zlib",
        "Unicode-3.0",
    ] {
        assert!(
            allow.iter().any(|l| l == required),
            "allow list lacks {required}"
        );
    }
    let copyleft = Regex::new(r"GPL|SSPL|BUSL|MPL").unwrap();
    assert!(
        !allow.iter().any(|l| copyleft.is_match(l)),
        "copyleft license allowed: {allow:?}"
    );
}

#[test]
fn deny_toml_denies_unknown_sources() {
    let doc = toml_at("deny.toml");
    assert_eq!(doc["sources"]["unknown-registry"].as_str(), Some("deny"));
    assert_eq!(doc["sources"]["unknown-git"].as_str(), Some("deny"));
}

#[test]
fn ci_workflow_runs_the_fast_checks_on_every_pr() {
    assert_contains_all(
        ".github/workflows/ci.yml",
        &[
            "pull_request",
            "ubuntu-24.04",
            "just lint",
            "just deny",
            "editor/src-tauri/deny.toml",
            "just wasm",
            "save-if",
        ],
    );
    let ci = std::fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).unwrap();
    for slow in ["macos-", "nextest run", "gate-tests"] {
        assert!(
            !ci.contains(slow),
            "ci.yml must stay fast (ADR-0071); found {slow:?}"
        );
    }
}

#[test]
fn vulkan_workflow_runs_the_backend_on_lavapipe_on_demand() {
    assert_contains_all(
        ".github/workflows/vulkan.yml",
        &[
            "workflow_dispatch",
            "ubuntu-24.04",
            "mesa-vulkan-drivers",
            "vulkan-validationlayers",
            "CONCORDIA_REQUIRE_VULKAN: \"1\"",
            "nextest run -p concordia-rhi-vulkan",
        ],
    );
    let workflow =
        std::fs::read_to_string(repo_root().join(".github/workflows/vulkan.yml")).unwrap();
    for automatic in ["pull_request", "push:", "schedule"] {
        assert!(
            !workflow.contains(automatic),
            "the Vulkan workflow runs only on demand (author decision 2026-11-25); found {automatic:?}"
        );
    }
}

#[test]
fn regression_workflow_covers_the_full_suite_on_demand() {
    assert_contains_all(
        ".github/workflows/regression.yml",
        &[
            "workflow_dispatch",
            "macos-",
            "ubuntu-24.04",
            "nextest run",
            "--doc",
            "just wasm",
            "just gate-tests M-1",
            "just gate-tests M0",
            "just gate-tests M1",
            "just gate-tests M3S",
            "pnpm e2e",
            "editor/src-tauri/Cargo.toml",
        ],
    );
}

#[test]
fn dco_workflow_checks_sign_off() {
    assert_contains_all(
        ".github/workflows/dco.yml",
        &["pull_request", "Signed-off-by"],
    );
}

#[test]
fn no_config_requires_llm_api_key() {
    let key =
        Regex::new(r"ANTHROPIC_API_KEY|OPENAI_API_KEY|secrets\.[A-Z_]*(API|LLM|KEY)").unwrap();
    let is_config = |f: &str| {
        f.starts_with(".github/")
            || f.starts_with(".claude/")
            || f == "justfile"
            || f == "deny.toml"
            || f.ends_with("Cargo.toml")
            || (f.starts_with("tools/") && f.ends_with(".sh"))
    };
    for file in tracked_files()
        .into_iter()
        .filter(|f| is_config(f) && f != "tools/doctor.sh")
    {
        assert!(
            !key.is_match(&read_repo_file(&file)),
            "{file} references an LLM API key"
        );
    }
}

fn claude_settings() -> serde_json::Value {
    serde_json::from_str(&read_repo_file(".claude/settings.json"))
        .expect(".claude/settings.json is JSON")
}

fn permission_rules(kind: &str) -> Vec<String> {
    claude_settings()["permissions"][kind]
        .as_array()
        .map(|rules| {
            rules
                .iter()
                .filter_map(|r| r.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn claude_settings_is_valid_json() {
    assert!(claude_settings().is_object());
}

#[test]
fn claude_settings_allows_common_read_only_and_test_commands() {
    let allow = permission_rules("allow");
    for rule in [
        "Bash(cargo nextest run:*)",
        "Bash(cargo test:*)",
        "Bash(cargo clippy:*)",
        "Bash(cargo fmt --all --check)",
        "Bash(just test)",
        "Bash(just lint)",
        "Bash(just gate:*)",
        "Bash(just doctor)",
        "Bash(git status:*)",
        "Bash(git diff:*)",
        "Bash(git log:*)",
    ] {
        assert!(allow.iter().any(|r| r == rule), "allow lacks {rule}");
    }
}

#[test]
fn claude_settings_never_auto_allows_pushes_or_installs() {
    let risky = Regex::new(r"git push|brew install|cargo install|rustup self|curl|rm -rf").unwrap();
    assert!(!permission_rules("allow").iter().any(|r| risky.is_match(r)));
    assert!(
        permission_rules("ask")
            .iter()
            .any(|r| r == "Bash(git push:*)")
    );
}

#[test]
fn claude_settings_sets_no_api_key_env() {
    if let Some(env) = claude_settings()["env"].as_object() {
        assert!(
            !env.keys().any(|k| k.contains("API_KEY")),
            "settings env sets an API key"
        );
    }
}

/// Files that may still mention the pre-Terra crate name (history: ADRs and plans that record it).
const OLD_ECS_NAME_ALLOWED: [&str; 3] = [
    "plans/M0-headless-core.md",
    "plans/M7-terra.md",
    "plans/README.md",
];

#[test]
fn ecs_is_called_terra_everywhere() {
    let old = Regex::new(r"concordia[-_]ecs").unwrap();
    let offenders: Vec<String> = tracked_files()
        .into_iter()
        .filter(|f| !f.starts_with("docs/adr/") && !OLD_ECS_NAME_ALLOWED.contains(&f.as_str()))
        .filter(|f| {
            std::fs::read_to_string(repo_root().join(f)).is_ok_and(|text| old.is_match(&text))
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "rename concordia-terra to concordia-terra in: {offenders:?}"
    );
}

/// Goldens stay in plain git (decision D5, ADR-0042): small files, bounded total.
#[test]
fn golden_images_stay_small() {
    const MAX_FILE: u64 = 1 << 20;
    const MAX_TOTAL: u64 = 50 << 20;
    let mut total = 0;
    let mut stack = vec![repo_root().join("tests/golden")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "png") {
                let size = entry.metadata().unwrap().len();
                assert!(
                    size <= MAX_FILE,
                    "{} is {size} bytes (limit {MAX_FILE})",
                    path.display()
                );
                total += size;
            }
        }
    }
    assert!(
        total <= MAX_TOTAL,
        "goldens total {total} bytes (limit {MAX_TOTAL}); time for Git LFS?"
    );
}
