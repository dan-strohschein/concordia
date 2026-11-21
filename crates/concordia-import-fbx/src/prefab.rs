//! FBX nodes → a `ModelTree` (the implicit root is the prefab's root entity).

use concordia_assets::{AssetError, ImportOutput, SubIds};
use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};
use concordia_import_model::{MeshPart, ModelNode, ModelTree, write_prefab};
use concordia_scene::builtin::Transform;

use crate::ffi::{Mesh, Node};
use crate::mesh::part_sub;
use crate::names::Names;

/// Builds the prefab and returns each FBX node's entity id (`None` for the implicit root).
pub(crate) fn prefab(
    nodes: &[Node],
    meshes: &[Mesh],
    names: &Names,
    stem: &str,
    sub_ids: &SubIds,
    out: &mut ImportOutput,
) -> Result<Vec<Option<EntityId>>, AssetError> {
    let index = model_indices(nodes);
    let ids = write_prefab(&tree(nodes, &index, meshes, names), stem, sub_ids, out)?;
    Ok(index.iter().map(|m| m.map(|m| ids[m])).collect())
}

/// Each FBX node's index in the tree (`None` for the implicit root, which is skipped).
fn model_indices(nodes: &[Node]) -> Vec<Option<usize>> {
    nodes
        .iter()
        .scan(0, |next, n| {
            Some((!n.is_root).then(|| std::mem::replace(next, *next + 1)))
        })
        .collect()
}

fn tree(nodes: &[Node], index: &[Option<usize>], meshes: &[Mesh], names: &Names) -> ModelTree {
    let parent_of = |n: &Node| n.parent.and_then(|p| index.get(p).copied().flatten());
    let mut tree = ModelTree::default();
    for (i, node) in nodes.iter().enumerate().filter(|(_, n)| !n.is_root) {
        if parent_of(node).is_none() {
            tree.roots.push(tree.nodes.len());
        }
        tree.nodes.push(ModelNode {
            name: names.nodes[i].clone(),
            transform: transform(node),
            children: Vec::new(),
            parts: parts(node, meshes, names),
        });
    }
    for (i, node) in nodes.iter().enumerate() {
        if let (Some(child), Some(parent)) = (index[i], parent_of(node)) {
            tree.nodes[parent].children.push(child);
        }
    }
    tree
}

fn transform(node: &Node) -> Transform {
    Transform {
        translation: Vec3::from_array(node.translation),
        rotation: Quat::from_array(node.rotation).normalize(),
        scale: Vec3::from_array(node.scale),
    }
}

/// One renderer per non-empty material part, with the node's own material in that slot.
fn parts(node: &Node, meshes: &[Mesh], names: &Names) -> Vec<MeshPart> {
    let Some((m, mesh)) = node.mesh.and_then(|m| Some((m, meshes.get(m)?))) else {
        return Vec::new();
    };
    (0..mesh.triangles.len())
        .filter(|p| mesh.triangles[*p] > 0)
        .map(|p| MeshPart {
            mesh: part_sub(names, m, p),
            material: node
                .materials
                .get(p)
                .copied()
                .flatten()
                .and_then(|i| names.materials.get(i))
                .map(|name| format!("material/{name}")),
            skin: mesh.skin.map(|s| format!("skin/{s}")),
        })
        .collect()
}
