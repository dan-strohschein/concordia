//! `.material.ron` import (M1 gate scene): materials are text files with a stable asset id.

mod common;

use std::path::Path;

use common::Project;
use concordia_assets::{AssetError, AssetRef, MaterialData};

const RED: &str =
    "(\n    base_color: (0.8, 0.1, 0.1, 1.0),\n    metallic: 0.0,\n    roughness: 0.7,\n)\n";

#[test]
fn material_ron_imports_as_a_material_with_a_stable_id() {
    let mut p = Project::new();
    std::fs::create_dir_all(p.path("materials")).unwrap();
    std::fs::write(p.path("materials/red.material.ron"), RED).unwrap();
    let path = Path::new("materials/red.material.ron");
    assert_eq!(p.db.importer_name(path), Some("material"));
    let report = p.db.import(path).unwrap();
    let reference: AssetRef = format!("asset:{}", report.id).parse().unwrap();
    let material = p.db.load_material(&reference).unwrap();
    assert_eq!(
        material,
        MaterialData {
            base_color: [0.8, 0.1, 0.1, 1.0],
            metallic: 0.0,
            roughness: 0.7,
            ..MaterialData::default()
        }
    );
    let again = p.db.import(path).unwrap();
    assert_eq!(again.id, report.id, "the sidecar keeps the id");
    assert!(again.cached);
}

#[test]
fn scenes_and_prefabs_are_not_claimed_by_the_material_importer() {
    let p = Project::new();
    assert_eq!(p.db.importer_name(Path::new("scenes/a.scene.ron")), None);
    assert_eq!(p.db.importer_name(Path::new("a.prefab.ron")), None);
    assert_eq!(p.db.importer_name(Path::new("a.PNG")), Some("image"));
}

#[test]
fn an_invalid_material_names_the_file_and_the_problem() {
    let mut p = Project::new();
    std::fs::write(
        p.path("bad.material.ron"),
        "(base_color: (1.0, 0.0), shiny: true)",
    )
    .unwrap();
    match p.db.import(Path::new("bad.material.ron")) {
        Err(AssetError::Import { path, reason }) => {
            assert_eq!(path, Path::new("bad.material.ron"));
            assert!(!reason.is_empty());
        }
        other => panic!("expected an import error, got {other:?}"),
    }
}
