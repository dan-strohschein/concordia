//! FBX imports with meshlets (M4 4.2): static parts get them, skinned ones refuse `true`.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use concordia_assets::{
    AssetError, ImportInput, ImportOutput, Importer, MeshData, NoDerivedCache, SubIds, decode,
    import_jobs,
};
use concordia_cluster::MeshletMesh;
use concordia_core::id::{AssetId, TypedId, Uuid};
use concordia_import_fbx::FbxImporter;

fn import(file: &str, settings: serde_json::Value) -> Result<ImportOutput, AssetError> {
    let bytes = std::fs::read(common::fixture(file)).unwrap();
    let parent =
        AssetId::try_from_uuid(Uuid::new_v5(&Uuid::NAMESPACE_OID, b"fbx-meshlets")).unwrap();
    let mut sub_ids = SubIds::new(parent, BTreeMap::new());
    let read = |n: &str| -> Result<Vec<u8>, AssetError> { Err(AssetError::Io(n.into())) };
    FbxImporter.import(ImportInput {
        path: Path::new(file),
        bytes: &bytes,
        settings: &settings,
        read_relative: &read,
        sub_ids: &mut sub_ids,
        derived: &NoDerivedCache,
        jobs: import_jobs().unwrap(),
    })
}

#[test]
fn fbx_mesh_gets_meshlets() {
    let out = import("static_quad.fbx", serde_json::json!({ "meshlets": true })).unwrap();
    let mut seen = 0;
    for (sub, artifact) in out.artifacts.iter().filter(|(k, _)| k.starts_with("mesh/")) {
        let mesh: MeshData = decode(&artifact.bytes).unwrap();
        let m = MeshletMesh::decode(&mesh.meshlets).unwrap_or_else(|e| panic!("{sub}: {e}"));
        assert_eq!(m.source_triangles as usize, mesh.indices.len() / 3);
        seen += 1;
    }
    assert!(seen > 0);
    let auto = import("static_quad.fbx", serde_json::json!({})).unwrap();
    for artifact in auto
        .artifacts
        .values()
        .filter(|a| a.kind == concordia_assets::AssetKind::Mesh)
    {
        let mesh: MeshData = decode(&artifact.bytes).unwrap();
        assert!(
            mesh.meshlets.is_empty(),
            "a quad is under auto's 8k triangles"
        );
    }
}

#[test]
fn meshlets_true_on_a_skinned_fbx_is_an_error() {
    let err = import("skinned.fbx", serde_json::json!({ "meshlets": true })).unwrap_err();
    assert!(err.to_string().contains("skinned"), "{err}");
}
