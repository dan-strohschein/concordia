//! The node hierarchy → a `ModelTree`, written as the prefab by `concordia-import-model`.

use concordia_assets::{AssetError, ImportOutput, SubIds};
use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};
use concordia_import_model::{MeshPart, ModelNode, ModelTree, write_prefab};
use concordia_scene::builtin::Transform;

use crate::names::Names;

/// Builds the prefab and returns each node's entity id, by node index (for skins and clips).
pub(crate) fn prefab(
    doc: &gltf::Document,
    names: &Names,
    stem: &str,
    sub_ids: &SubIds,
    out: &mut ImportOutput,
) -> Result<Vec<EntityId>, AssetError> {
    let scene = doc.default_scene().or_else(|| doc.scenes().next());
    let tree = ModelTree {
        nodes: doc.nodes().map(|n| node(&n, names)).collect(),
        roots: scene
            .iter()
            .flat_map(|s| s.nodes())
            .map(|n| n.index())
            .collect(),
    };
    write_prefab(&tree, stem, sub_ids, out)
}

fn node(node: &gltf::Node<'_>, names: &Names) -> ModelNode {
    let (t, r, s) = node.transform().decomposed();
    ModelNode {
        name: names.nodes[node.index()].clone(),
        transform: Transform {
            translation: Vec3::from_array(t),
            rotation: Quat::from_array(r),
            scale: Vec3::from_array(s),
        },
        children: node.children().map(|c| c.index()).collect(),
        parts: node.mesh().map_or_else(Vec::new, |m| parts(&m, names)),
    }
}

fn parts(mesh: &gltf::Mesh<'_>, names: &Names) -> Vec<MeshPart> {
    mesh.primitives()
        .enumerate()
        .map(|(p, primitive)| MeshPart {
            mesh: format!("mesh/{}/{p}", names.meshes[mesh.index()]),
            material: primitive
                .material()
                .index()
                .map(|i| format!("material/{}", names.materials[i])),
        })
        .collect()
}
