//! Scenes with script-defined (dynamic) components (M2 phase 2.6a, ADR-0062).

mod common;

use std::collections::BTreeMap;

use common::{Gadget, Inner, Mode, registry};
use concordia_core::id::{DeterministicIdGen, IdSource};
use concordia_core::math::{Vec2, Vec3};
use concordia_reflect::{Reflect, TypeRegistry};
use concordia_scene::{SceneMeta, load_into_world, read_scene, snapshot_world, write_scene};
use concordia_terra::{World, register_dynamic_component};

fn rich_gadget() -> Gadget {
    Gadget {
        mode: Mode::Patrol {
            points: vec![Vec3::new(0.1, -0.0, 3.0), Vec3::ONE],
            looping: true,
        },
        label: Some("lamp \"post\"".into()),
        tags: vec!["a".into(), "b c".into()],
        stats: BTreeMap::from([("speed".into(), 1.5), ("armor".into(), -2.0)]),
        offset: Some(Vec2::new(0.5, 0.25)),
        count: 7,
        inner: Inner {
            a: -42,
            note: "nested".into(),
        },
        pair: (3, true),
    }
}

/// A registry where `Gadget` is a dynamic type with the static `Gadget`'s shape.
fn script_registry() -> TypeRegistry {
    let mut reg = TypeRegistry::new();
    concordia_scene::builtin::register(&mut reg).unwrap();
    reg.register::<Mode>().unwrap();
    reg.register::<Inner>().unwrap();
    register_dynamic_component(&mut reg, &Gadget::type_info().to_desc()).unwrap();
    reg
}

fn scene_text(world: &World) -> String {
    let meta = SceneMeta {
        id: DeterministicIdGen::new(9).next_id().unwrap(),
        name: "Dynamic".into(),
    };
    write_scene(&snapshot_world(&meta, world).unwrap()).unwrap()
}

#[test]
fn scene_with_dynamic_components_roundtrips_byte_identical() {
    let id = DeterministicIdGen::new(1).next_id().unwrap();
    let gadget = rich_gadget();

    let mut typed = World::new();
    typed.spawn_with_id(id).unwrap();
    typed.insert(id, gadget.clone()).unwrap();
    let typed_text = scene_text(&typed);

    let reg = script_registry();
    let mut dynamic = World::new();
    dynamic.spawn_with_id(id).unwrap();
    dynamic
        .set_json(id, "Gadget", serde_json::to_value(&gadget).unwrap(), &reg)
        .unwrap();
    let text = scene_text(&dynamic);
    assert_eq!(
        text, typed_text,
        "a dynamic twin saves exactly like the Rust type"
    );

    let doc = read_scene(&text, &reg).unwrap();
    assert_eq!(write_scene(&doc).unwrap(), text);
    let mut loaded = World::new();
    load_into_world(&doc, &mut loaded, &reg).unwrap();
    assert_eq!(loaded.state_hash().unwrap(), dynamic.state_hash().unwrap());
    assert_eq!(loaded.state_hash().unwrap(), typed.state_hash().unwrap());

    // The static registry reads the same text into the Rust type.
    let static_doc = read_scene(&text, &registry()).unwrap();
    let mut back = World::new();
    load_into_world(&static_doc, &mut back, &registry()).unwrap();
    assert_eq!(back.get::<Gadget>(id), Some(&gadget));
}
