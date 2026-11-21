//! Format-neutral import: skin weight reduction, names, mesh finishing and the prefab.

use concordia_assets::{ImportOutput, MeshData, SubIds};
use concordia_core::id::{AssetId, TypedId};
use concordia_import_model::{
    MAX_INFLUENCES, MeshPart, ModelNode, ModelTree, finish_mesh, reduce_influences, unique_names,
    write_prefab,
};
use concordia_scene::builtin::Transform;

fn sum(w: &[f32; 4]) -> f32 {
    w.iter().sum()
}

#[test]
fn influences_keep_the_four_largest_and_sum_to_one() {
    let (joints, weights) = reduce_influences(&[
        (6, 1.0),
        (0, 0.4),
        (1, 0.3),
        (2, 0.15),
        (3, 0.1),
        (4, 0.03),
        (5, 0.02),
    ]);
    assert_eq!(joints, [6, 0, 1, 2]);
    let total = 1.0 + 0.4 + 0.3 + 0.15;
    for (w, raw) in weights.iter().zip([1.0, 0.4, 0.3, 0.15]) {
        assert!((w - raw / total).abs() < 1e-6, "{weights:?}");
    }
    assert!((sum(&weights) - 1.0).abs() < 1e-6);
    assert_eq!(MAX_INFLUENCES, 4);
}

#[test]
fn unsorted_and_unnormalized_influences_are_sorted_then_normalized() {
    let (joints, weights) = reduce_influences(&[(2, 0.75), (1, 0.75), (9, 1.5)]);
    assert_eq!(
        joints,
        [9, 1, 2, 0],
        "largest first; ties keep the lower joint"
    );
    assert_eq!(weights, [0.5, 0.25, 0.25, 0.0]);
}

#[test]
fn invalid_influences_are_dropped_and_none_follow_joint_zero() {
    let (joints, weights) = reduce_influences(&[
        (3, f32::NAN),
        (4, -1.0),
        (5, 0.0),
        (7, 2.0),
        (8, f32::INFINITY),
    ]);
    assert_eq!((joints, weights), ([7, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]));
    assert_eq!(reduce_influences(&[]), ([0; 4], [1.0, 0.0, 0.0, 0.0]));
    assert_eq!(
        reduce_influences(&[(2, f32::NAN)]),
        ([0; 4], [1.0, 0.0, 0.0, 0.0])
    );
}

#[test]
fn names_use_the_source_then_indices_and_mark_duplicates() {
    let names = unique_names([
        Some("Body"),
        None,
        Some("Arm"),
        Some("Arm"),
        Some(""),
        Some("a/b"),
    ]);
    assert_eq!(names, ["Body", "1", "Arm#2", "Arm#3", "4", "a_b"]);
}

#[test]
fn finishing_fills_normals_uvs_tangents_and_bounds() {
    let mut mesh = MeshData {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]],
        indices: vec![0, 1, 2],
        ..MeshData::default()
    };
    finish_mesh(&mut mesh).unwrap();
    assert_eq!(mesh.normals, vec![[0.0, 1.0, 0.0]; 3]);
    assert_eq!((mesh.uv0.len(), mesh.tangents.len()), (3, 3));
    assert_eq!(mesh.aabb, ([0.0, 0.0, -1.0], [1.0, 0.0, 0.0]));
    let mut broken = MeshData {
        positions: vec![[0.0; 3]],
        indices: vec![0, 0, 9],
        ..MeshData::default()
    };
    assert!(
        finish_mesh(&mut broken)
            .unwrap_err()
            .contains("index out of range")
    );
}

fn node(name: &str, children: Vec<usize>, parts: Vec<MeshPart>) -> ModelNode {
    ModelNode {
        name: name.to_owned(),
        transform: Transform::default(),
        children,
        parts,
    }
}

#[test]
fn prefab_ids_follow_names_and_extra_parts_become_children() {
    let part = |p: usize| MeshPart {
        mesh: format!("mesh/Body/{p}"),
        material: (p == 1).then(|| "material/Skin".to_owned()),
        skin: None,
    };
    let tree = ModelTree {
        nodes: vec![
            node("Hips", vec![], vec![]),
            node("Body", vec![0], vec![part(0), part(1)]),
        ],
        roots: vec![1],
    };
    let asset = test_asset();
    let mut out = ImportOutput::default();
    let ids = write_prefab(
        &tree,
        "char",
        &SubIds::new(asset, Default::default()),
        &mut out,
    )
    .unwrap();
    let swapped = ModelTree {
        nodes: vec![tree.nodes[1].clone(), node("Hips", vec![], vec![])],
        roots: vec![0],
    };
    let mut again = ImportOutput::default();
    let mut swapped_tree = swapped;
    swapped_tree.nodes[0].children = vec![1];
    let moved = write_prefab(
        &swapped_tree,
        "char",
        &SubIds::new(asset, Default::default()),
        &mut again,
    )
    .unwrap();
    assert_eq!(
        (ids[0], ids[1]),
        (moved[1], moved[0]),
        "ids come from names, not order"
    );
    assert_eq!(
        out.artifacts[""], again.artifacts[""],
        "same tree, same prefab text"
    );
    let text = String::from_utf8(out.artifacts[""].bytes.clone()).unwrap();
    assert!(text.contains("Body primitive 1"), "{text}");
    assert!(
        text.contains(&format!("asset:{asset}#material/Skin")),
        "{text}"
    );
    assert!(text.contains("builtin:material/default"), "{text}");
}

#[test]
fn skinned_parts_get_a_skinned_component_naming_their_skin() {
    let part = |p: usize, skin: Option<&str>| MeshPart {
        mesh: format!("mesh/M/{p}"),
        material: None,
        skin: skin.map(str::to_owned),
    };
    let tree = ModelTree {
        nodes: vec![
            node(
                "Body",
                vec![],
                vec![part(0, Some("skin/0")), part(1, Some("skin/0"))],
            ),
            node("Prop", vec![], vec![part(0, None)]),
        ],
        roots: vec![0, 1],
    };
    let asset = test_asset();
    let mut out = ImportOutput::default();
    write_prefab(
        &tree,
        "char",
        &SubIds::new(asset, Default::default()),
        &mut out,
    )
    .unwrap();
    let mut registry = concordia_reflect::TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    concordia_assets::register_types(&mut registry).unwrap();
    let text = String::from_utf8(out.artifacts[""].bytes.clone()).unwrap();
    let doc = concordia_scene::read_prefab(&text, &registry).unwrap();
    let skin_of = |name: &str| {
        let entity = doc
            .entities
            .iter()
            .find(|e| e.components["Name"].to_json().unwrap() == name)
            .unwrap_or_else(|| panic!("no {name} in {text}"));
        entity
            .components
            .get("Skinned")
            .map(|c| c.to_json().unwrap()["skin"].clone())
    };
    let skin = serde_json::json!(format!("asset:{asset}#skin/0"));
    assert_eq!(skin_of("Body"), Some(skin.clone()));
    assert_eq!(
        skin_of("Body primitive 1"),
        Some(skin),
        "every part of a skinned mesh"
    );
    assert_eq!(skin_of("Prop"), None);
}

#[test]
fn a_node_reached_twice_or_out_of_range_is_an_error() {
    let asset = test_asset();
    let sub_ids = SubIds::new(asset, Default::default());
    let twice = ModelTree {
        nodes: vec![node("A", vec![1], vec![]), node("B", vec![], vec![])],
        roots: vec![0, 1],
    };
    let err = write_prefab(&twice, "x", &sub_ids, &mut ImportOutput::default()).unwrap_err();
    assert!(err.to_string().contains("reached twice"), "{err}");
    let missing = ModelTree {
        nodes: vec![node("A", vec![5], vec![])],
        roots: vec![0],
    };
    let err = write_prefab(&missing, "x", &sub_ids, &mut ImportOutput::default()).unwrap_err();
    assert!(err.to_string().contains("out of range"), "{err}");
}

fn test_asset() -> AssetId {
    AssetId::try_from_uuid(uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_OID,
        b"model-test",
    ))
    .unwrap()
}
