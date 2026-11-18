//! Byte-stable round trips between text, documents and worlds (M0 phase 0.5).

mod common;

use common::{canonical_fixtures, fixture, registry};
use concordia_core::id::{DeterministicIdGen, IdSource};
use concordia_core::math::{Quat, Vec3};
use concordia_ecs::World;
use concordia_scene::builtin::{Name, Parent, Transform, Velocity};
use concordia_scene::{SceneMeta, load_into_world, read_scene, snapshot_world, write_scene};

#[test]
fn load_then_save_is_byte_identical_for_canonical_fixtures() {
    let reg = registry();
    let fixtures = canonical_fixtures();
    assert!(fixtures.len() >= 2);
    for (name, text) in fixtures {
        let doc = read_scene(&text, &reg).unwrap_or_else(|e| panic!("{name}: {e}"));
        let written = write_scene(&doc).unwrap();
        assert_eq!(written, text, "{name} is not byte-stable");
    }
}

#[test]
fn save_then_load_reproduces_world_hash() {
    let reg = registry();
    let mut ids = DeterministicIdGen::new(3);
    let mut world = World::new();
    let player = world.spawn(&mut ids).unwrap();
    world.insert(player, Name("Player".into())).unwrap();
    world
        .insert(
            player,
            Transform {
                translation: Vec3::new(0.1, -0.0, 3.0),
                rotation: Quat::from_rotation_y(0.7),
                scale: Vec3::ONE,
            },
        )
        .unwrap();
    world
        .insert(
            player,
            Velocity {
                linear: Vec3::X,
                angular: Vec3::ZERO,
            },
        )
        .unwrap();
    let child = world.spawn(&mut ids).unwrap();
    world.insert(child, Parent(player)).unwrap();

    let meta = SceneMeta {
        id: ids.next_id().unwrap(),
        name: "Test".into(),
    };
    let text = write_scene(&snapshot_world(&meta, &world).unwrap()).unwrap();
    let doc = read_scene(&text, &reg).unwrap();
    let mut loaded = World::new();
    load_into_world(&doc, &mut loaded, &reg).unwrap();
    assert_eq!(loaded.state_hash().unwrap(), world.state_hash().unwrap());
}

#[test]
fn stable_ids_survive_roundtrip() {
    let reg = registry();
    let doc = read_scene(&fixture("builtins.scene.ron"), &reg).unwrap();
    assert_eq!(doc.id.to_string(), "0193a1c2-7e40-7000-8000-000000000001");
    let mut world = World::new();
    load_into_world(&doc, &mut world, &reg).unwrap();
    let again = snapshot_world(&doc.meta(), &world).unwrap();
    assert_eq!(again, doc);
    let ids: Vec<String> = world
        .entity_ids_sorted()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(ids[0], "0193a1c2-7e40-7000-8000-000000000002");
}
