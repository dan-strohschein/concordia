//! `project.files`: the editor's FileSystem dock lists project files through the Engine API (M1 phase 1.11).

mod common;

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use concordia_api::{Engine, EngineConfig};
use concordia_assets::AssetsModule;
use serde_json::{Value, json};

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn engine(root: &Path) -> (Engine, concordia_core::id::SessionId) {
    let mut config = EngineConfig::new(root);
    config.threads = NonZeroUsize::MIN;
    config.modules = vec![Arc::new(AssetsModule::new(root))];
    let mut engine = Engine::new(config).unwrap();
    let session = engine.open_session();
    (engine, session)
}

fn paths(listing: &Value) -> Vec<&str> {
    listing["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect()
}

#[test]
fn project_files_lists_project_relative_paths_and_skips_engine_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "concordia.toml", "[project]\nname = \"p\"\n");
    write(root, "scenes/main.scene.ron", "()");
    write(root, "notes.txt", "hi");
    write(root, "materials/red.material.ron", "(metallic: 0.0)");
    write(root, ".gitignore", "build/\n");
    write(root, "build/out.bin", "x");
    write(root, ".git/HEAD", "ref");
    common::write_png(&root.join("tex/crate.png"), 8, 0);
    let (mut engine, s) = engine(root);
    let imported = engine
        .call(s, "asset.import", json!({"path": "tex/crate.png"}))
        .unwrap();
    // The import wrote a sidecar and filled `.concordia/ddc`; neither is a project file.
    let listing = engine.call(s, "project.files", json!({})).unwrap();
    assert_eq!(
        paths(&listing),
        [
            "concordia.toml",
            "materials/red.material.ron",
            "notes.txt",
            "scenes/main.scene.ron",
            "tex/crate.png"
        ],
        "{listing}"
    );
    let file = |path: &str| {
        listing["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"] == path)
            .unwrap()
            .clone()
    };
    assert_eq!(file("scenes/main.scene.ron")["kind"], "scene");
    assert_eq!(file("notes.txt")["kind"], "other");
    assert_eq!(file("materials/red.material.ron")["kind"], "material");
    assert_eq!(file("materials/red.material.ron")["importer"], "material");
    let png = file("tex/crate.png");
    assert_eq!(png["kind"], "image");
    assert_eq!(
        png["asset"], imported["id"],
        "imported files carry their asset id"
    );
    assert_eq!(png["importer"], "image");
    assert!(png["size"].as_u64().unwrap() > 0);
    assert!(file("notes.txt")["asset"].is_null());
    assert!(
        file("notes.txt")["importer"].is_null(),
        "nothing imports .txt"
    );

    let sub = engine
        .call(s, "project.files", json!({"dir": "tex"}))
        .unwrap();
    assert_eq!(paths(&sub), ["tex/crate.png"]);
    for bad in ["../", "/etc", "missing"] {
        let err = engine
            .call(s, "project.files", json!({"dir": bad}))
            .unwrap_err();
        assert_eq!(err.code(), -32602, "{bad}: {err:?}");
    }
}
