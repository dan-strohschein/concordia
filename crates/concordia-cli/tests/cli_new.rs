//! `concordia new` (M0 phase 0.7).

mod common;

use common::{run, stderr};
use concordia_reflect::TypeRegistry;
use serde_json::Value;

fn registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    registry
}

#[test]
fn concordia_new_creates_project_skeleton() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(&["new", "game", "--name", "My Game"], dir.path());
    assert!(output.status.success(), "{}", stderr(&output));
    let root = dir.path().join("game");
    let manifest = std::fs::read_to_string(root.join("concordia.toml")).unwrap();
    assert!(
        manifest.contains("name = \"My Game\"")
            && manifest.contains("main = \"scenes/main.scene.ron\""),
        "{manifest}"
    );
    let scene = std::fs::read_to_string(root.join("scenes/main.scene.ron")).unwrap();
    assert_eq!(
        concordia_scene::format(&scene, &registry()).unwrap(),
        scene,
        "the main scene is canonical"
    );
    assert!(
        std::fs::read_to_string(root.join(".gitignore"))
            .unwrap()
            .lines()
            .any(|l| l == ".concordia/")
    );
    let mcp: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join(".mcp.json")).unwrap()).unwrap();
    // Changed deliberately in M3 3.3 (refinement G7): it names `concordia` on PATH, not this
    // machine's binary, so the committed file is portable and holds no user path.
    assert_eq!(mcp["mcpServers"]["concordia"]["command"], "concordia");
}

#[test]
fn concordia_new_mcp_json_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(&["new", "g"], dir.path()).status.success());
    let mcp: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("g/.mcp.json")).unwrap())
            .unwrap();
    insta::assert_json_snapshot!(mcp);
}

#[test]
fn concordia_new_refuses_non_empty_dir_without_force() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("keep.txt"), "mine").unwrap();
    let output = run(&["new", "."], dir.path());
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("--force"), "{}", stderr(&output));
    assert!(!dir.path().join("concordia.toml").exists());
    assert!(run(&["new", ".", "--force"], dir.path()).status.success());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("keep.txt")).unwrap(),
        "mine",
        "other files are kept"
    );
}

/// Every file `concordia new` wrote under `root`, project-relative and sorted.
fn files(root: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            match path.is_dir() {
                true => stack.push(path),
                false => out.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                ),
            }
        }
    }
    out.sort();
    out
}

#[test]
fn new_project_enables_modules_and_studio() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(&["new", "g"], dir.path()).status.success());
    let manifest = std::fs::read_to_string(dir.path().join("g/concordia.toml")).unwrap();
    for section in [
        "[physics]",
        "[nav]",
        "[assets]",
        "[anim]",
        "[audio]",
        "[ui]",
        "[render]",
        "[scripts]",
        "[replay]",
        "[studio]",
        "[input]\nmap = \"input/default.input.ron\"",
    ] {
        assert!(manifest.contains(section), "{section} missing:\n{manifest}");
    }
    // The starter map: a click and a pointer ray, the names ClickToMove uses by default.
    let map = std::fs::read_to_string(dir.path().join("g/input/default.input.ron")).unwrap();
    let map = concordia_input::InputMap::parse(&map).unwrap();
    assert_eq!(map, concordia_input::InputMap::starter());
    let defaults = concordia_nav::ClickToMove::default();
    assert!(map.action(&defaults.click).is_some() && map.action(&defaults.pointer).is_some());
}

/// The manifest documents the shadow settings (M4 4.7) as comments, so the defaults stay in force
/// and the file still parses.
#[test]
fn new_project_documents_the_shadow_settings() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(&["new", "g"], dir.path()).status.success());
    let manifest = std::fs::read_to_string(dir.path().join("g/concordia.toml")).unwrap();
    for line in [
        "# shadows = \"vsm\"",
        "# [render.vsm]",
        "# pool_pages = 4096",
        "# levels = 16",
        "# keep_frames = 60",
        "# cache = true",
    ] {
        assert!(manifest.contains(line), "{line} missing:\n{manifest}");
    }
    let parsed: toml::Table = toml::from_str(&manifest).unwrap();
    assert!(
        parsed["render"].as_table().unwrap().is_empty(),
        "only comments"
    );
}

#[test]
fn new_project_writes_the_default_studio() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(&["new", "g"], dir.path()).status.success());
    let root = dir.path().join("g");
    let all = files(&root);
    let agents: Vec<&String> = all
        .iter()
        .filter(|f| f.starts_with("studio/agents/"))
        .collect();
    assert_eq!(agents.len(), 12, "{all:?}");
    assert!(all.contains(&"studio/team.toml".to_owned()), "{all:?}");
    assert!(all.contains(&"studio/tasks/.gitkeep".to_owned()), "{all:?}");
    let ignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(
        ignore.lines().any(|l| l == ".claude/settings.local.json"),
        "{ignore}"
    );
}

#[test]
fn new_project_files_hold_no_user_paths() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(&["new", "g"], dir.path()).status.success());
    let root = dir.path().join("g");
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/".into());
    for file in files(&root) {
        let text = std::fs::read_to_string(root.join(&file)).unwrap_or_default();
        assert!(
            !text.contains(&home) && !text.contains(&*root.to_string_lossy()),
            "{file} holds a user path"
        );
    }
}

#[test]
fn new_no_studio_skips_the_studio_files() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        run(&["new", "g", "--no-studio"], dir.path())
            .status
            .success()
    );
    assert!(!dir.path().join("g/studio").exists());
    let manifest = std::fs::read_to_string(dir.path().join("g/concordia.toml")).unwrap();
    assert!(
        !manifest.contains("[studio]") && manifest.contains("[render]"),
        "{manifest}"
    );
}

#[test]
fn new_minimal_has_no_modules() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(&["new", "g", "--minimal"], dir.path()).status.success());
    let manifest = std::fs::read_to_string(dir.path().join("g/concordia.toml")).unwrap();
    for section in [
        "[physics]",
        "[assets]",
        "[nav]",
        "[anim]",
        "[audio]",
        "[ui]",
        "[render]",
        "[scripts]",
        "[replay]",
        "[studio]",
        "[input]",
    ] {
        assert!(
            !manifest.contains(section),
            "{section} present:\n{manifest}"
        );
    }
    assert!(!dir.path().join("g/studio").exists());
}
