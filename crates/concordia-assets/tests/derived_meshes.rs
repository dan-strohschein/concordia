//! Meshes made from other derived data at runtime (M8 8.5b: a breakable's baked pieces), held by
//! the asset database under their own reference, and `PieceRenderer`, which draws them.

mod common;

use common::Project;
use concordia_assets::{AssetRef, AssetSource, MeshRenderer, PieceRenderer, shapes};

fn derived() -> AssetRef {
    "asset:0193a1c2-7e40-7000-8000-0000000000d1#mesh/surface"
        .parse()
        .unwrap()
}

#[test]
fn derived_meshes_load_by_their_reference() {
    let mut p = Project::new();
    let cube = shapes::cube([1.0, 2.0, 3.0]);
    assert!(p.db.load_mesh(&derived()).is_err(), "nothing there yet");
    p.db.insert_derived_mesh(derived(), cube.clone()).unwrap();
    assert_eq!(p.db.load_mesh(&derived()).unwrap(), cube);
    assert_eq!(AssetSource::mesh(&mut p.db, &derived()).unwrap(), cube);
}

#[test]
fn derived_meshes_must_be_valid_and_never_shadow_built_ins() {
    let mut p = Project::new();
    let mut broken = shapes::cube([1.0; 3]);
    broken.normals.pop();
    assert!(p.db.insert_derived_mesh(derived(), broken).is_err());
    assert!(p.db.load_mesh(&derived()).is_err(), "nothing was stored");
    let builtin: AssetRef = "builtin:mesh/cube".parse().unwrap();
    assert!(
        p.db.insert_derived_mesh(builtin.clone(), shapes::cube([5.0; 3]))
            .is_err()
    );
    assert_eq!(p.db.load_mesh(&builtin).unwrap(), shapes::cube([1.0; 3]));
}

#[test]
fn piece_renderer_roundtrips_and_is_registered() {
    let r = PieceRenderer {
        meshes: vec![MeshRenderer {
            mesh: derived(),
            ..MeshRenderer::default()
        }],
        pieces: 4,
        shown: vec![1, 3],
    };
    let value = serde_json::to_value(&r).unwrap();
    assert_eq!(serde_json::from_value::<PieceRenderer>(value).unwrap(), r);
    let mut registry = concordia_reflect::TypeRegistry::new();
    concordia_assets::register_types(&mut registry).unwrap();
    assert!(registry.get::<PieceRenderer>().is_some());
}
