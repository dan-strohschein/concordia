//! Prefabs: byte-stable files and flattened instantiation with fresh ids (M0 phase 0.5).

mod common;

use common::{fixture, registry};
use concordia_command::EditOp;
use concordia_core::id::DeterministicIdGen;
use concordia_scene::{instantiate_prefab, read_prefab, write_prefab};

#[test]
fn prefab_roundtrip_byte_identical() {
    let text = fixture("crates.prefab.ron");
    let prefab = read_prefab(&text, &registry()).unwrap();
    assert_eq!(prefab.name, "Crate Stack");
    assert_eq!(write_prefab(&prefab).unwrap(), text);
}

#[test]
fn instantiate_prefab_assigns_fresh_ids_and_remaps_parent_refs() {
    let prefab = read_prefab(&fixture("crates.prefab.ron"), &registry()).unwrap();
    let (root, ops) = instantiate_prefab(&prefab, &mut DeterministicIdGen::new(4)).unwrap();
    let again = instantiate_prefab(&prefab, &mut DeterministicIdGen::new(4)).unwrap();
    assert_eq!(
        (root, &ops),
        (again.0, &again.1),
        "deterministic with a deterministic id source"
    );

    let spawned: Vec<_> = ops
        .iter()
        .map(|op| match op {
            EditOp::Spawn { id, components } => (*id, components.clone()),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(spawned.len(), 2);
    let originals: Vec<String> = prefab.entities.iter().map(|e| e.id.to_string()).collect();
    for (id, _) in &spawned {
        assert!(!originals.contains(&id.to_string()), "ids are fresh");
    }
    assert_eq!(spawned[0].0, root, "root comes first");
    assert_eq!(
        spawned[1].1["Parent"],
        serde_json::json!(root.to_string()),
        "Parent remapped to the new root"
    );
}
