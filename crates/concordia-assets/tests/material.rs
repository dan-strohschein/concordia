//! `.material.ron` import (M1 gate scene; layered since M2 2.3b, ADR-0068): materials are text
//! files with a stable asset id, imported as layered `Material`s (M1 text migrates to one slab).

mod common;

use std::path::Path;

use common::Project;
use concordia_assets::{
    AssetError, AssetRef, Builtin, BuiltinAsset, Material, MaterialNode, SURFACE, Slab,
    read_material,
};

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
        Material::from_slab(Slab {
            base_color: [0.8, 0.1, 0.1, 1.0],
            metallic: 0.0,
            roughness: 0.7,
            ..Slab::default()
        }),
        "M1 text imports as one slab"
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

const CAR_PAINT: &str = r#"(
    nodes: {
        "base": Slab((base_color: (0.5, 0.02, 0.02, 1.0), roughness: 0.4)),
        "coat": Slab((base_color: (0.0, 0.0, 0.0, 1.0), roughness: 0.05, transmittance: (1.0, 1.0, 1.0))),
        "paint": Vertical(top: "coat", bottom: "base"),
    },
    root: "paint",
)
"#;

#[test]
fn layered_material_ron_imports_with_its_nodes() {
    let mut p = Project::new();
    std::fs::write(p.path("car.material.ron"), CAR_PAINT).unwrap();
    let report = p.db.import(Path::new("car.material.ron")).unwrap();
    let reference: AssetRef = format!("asset:{}", report.id).parse().unwrap();
    let material = p.db.load_material(&reference).unwrap();
    assert_eq!(material, read_material(CAR_PAINT).unwrap());
    assert_eq!(material.nodes.len(), 3);
    assert!(matches!(
        material.node("paint"),
        Some(MaterialNode::Vertical { .. })
    ));
}

#[test]
fn a_material_with_a_cycle_is_an_import_error_naming_it() {
    let mut p = Project::new();
    let text = r#"(nodes: {"a": Vertical(top: "b", bottom: "a"), "b": Slab(())}, root: "a")"#;
    std::fs::write(p.path("loop.material.ron"), text).unwrap();
    match p.db.import(Path::new("loop.material.ron")) {
        Err(AssetError::Import { reason, .. }) => {
            assert!(reason.contains("cycle"), "{reason}");
            assert!(reason.contains("a -> a"), "{reason}");
        }
        other => panic!("expected an import error, got {other:?}"),
    }
}

#[test]
fn the_builtin_default_material_is_m1s_white_dielectric_as_one_slab() {
    let reference: AssetRef = "builtin:material/default".parse().unwrap();
    let Some(BuiltinAsset::Material(material)) = Builtin::of(&reference) else {
        panic!("a built-in material");
    };
    assert_eq!(material.root, SURFACE);
    let white = Slab {
        base_color: [1.0; 4],
        metallic: 0.0,
        roughness: 0.5,
        ..Slab::default()
    };
    assert_eq!(material, Material::from_slab(white));
}
