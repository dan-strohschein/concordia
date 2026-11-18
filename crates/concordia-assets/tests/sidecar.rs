//! Sidecars and stable asset ids (M1 phase 1.4).

mod common;

use std::path::Path;

use common::Project;
use concordia_assets::{AssetError, AssetRef};

#[test]
fn first_import_writes_sidecar_with_new_asset_id() {
    let mut p = Project::new();
    p.png("textures/crate.png", 8, 0);
    let report = p.db.import(Path::new("textures/crate.png")).unwrap();
    let text = std::fs::read_to_string(p.path("textures/crate.png.import.ron")).unwrap();
    assert!(text.contains(&report.id.to_string()), "{text}");
    insta::assert_snapshot!(text.replace(&report.id.to_string(), "[id]"));
}

#[test]
fn reimport_preserves_asset_id_and_hits_the_ddc() {
    let mut p = Project::new();
    p.png("a.png", 8, 0);
    let first = p.db.import(Path::new("a.png")).unwrap();
    let second = p.db.import(Path::new("a.png")).unwrap();
    assert_eq!(first.id, second.id);
    assert!(!first.cached && second.cached, "{first:?} {second:?}");
}

#[test]
fn moving_file_with_sidecar_preserves_asset_id_and_refs() {
    let mut p = Project::new();
    p.png("old/tex.png", 8, 0);
    let id = p.db.import(Path::new("old/tex.png")).unwrap().id;
    let reference: AssetRef = format!("asset:{id}").parse().unwrap();
    std::fs::create_dir_all(p.path("new")).unwrap();
    std::fs::rename(p.path("old/tex.png"), p.path("new/tex.png")).unwrap();
    std::fs::rename(
        p.path("old/tex.png.import.ron"),
        p.path("new/tex.png.import.ron"),
    )
    .unwrap();
    p.db.scan().unwrap();
    assert_eq!(p.db.path_of(id), Some(Path::new("new/tex.png")));
    assert_eq!(p.db.load_texture(&reference).unwrap().width, 8);
}

#[test]
fn unknown_extensions_and_paths_outside_the_project_are_errors() {
    let mut p = Project::new();
    std::fs::write(p.path("notes.xyz"), "x").unwrap();
    assert!(matches!(
        p.db.import(Path::new("notes.xyz")),
        Err(AssetError::NoImporter { .. })
    ));
    assert!(matches!(
        p.db.import(Path::new("../escape.png")),
        Err(AssetError::Path(_))
    ));
    assert!(matches!(
        p.db.import(Path::new("missing.png")),
        Err(AssetError::Io(_))
    ));
}
