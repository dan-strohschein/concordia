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
    let command = mcp["mcpServers"]["concordia"]["command"].as_str().unwrap();
    assert!(std::path::Path::new(command).is_absolute(), "{command}");
}

#[test]
fn concordia_new_mcp_json_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run(&["new", "g"], dir.path()).status.success());
    let mcp: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("g/.mcp.json")).unwrap())
            .unwrap();
    insta::assert_json_snapshot!(mcp, { ".mcpServers.concordia.command" => "[concordia binary]" });
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
