//! Canonical formatting rules (M0 phase 0.5).

mod common;

use common::{Gadget, Health, Inner, Mode, fixture, registry};
use concordia_core::id::{DeterministicIdGen, IdSource};
use concordia_core::math::Vec3;
use concordia_scene::{SceneError, SceneMeta, format, read_scene, snapshot_world, write_scene};
use concordia_terra::World;

fn gadget(mode: Mode) -> Gadget {
    Gadget {
        mode,
        label: None,
        tags: vec![],
        stats: Default::default(),
        offset: None,
        count: 0,
        inner: Inner {
            a: 0,
            note: String::new(),
        },
        pair: (0, false),
    }
}

fn one_entity_scene(component: Gadget) -> String {
    let mut ids = DeterministicIdGen::new(1);
    let mut world = World::new();
    let id = world.spawn(&mut ids).unwrap();
    world.insert(id, component).unwrap();
    let meta = SceneMeta {
        id: ids.next_id().unwrap(),
        name: "S".into(),
    };
    write_scene(&snapshot_world(&meta, &world).unwrap()).unwrap()
}

#[test]
fn canonical_scene_snapshot() {
    let reg = registry();
    let doc = read_scene(&fixture("builtins.scene.ron"), &reg).unwrap();
    insta::assert_snapshot!(write_scene(&doc).unwrap());
    insta::assert_snapshot!(
        "all_variants",
        one_entity_scene(gadget(Mode::Jump(1.0, 2.0)))
    );
}

#[test]
fn format_is_idempotent() {
    let reg = registry();
    for name in [
        "builtins.scene.ron",
        "gadget.scene.ron",
        "noncanonical.scene.ron",
    ] {
        let once = format(&fixture(name), &reg).unwrap();
        assert_eq!(format(&once, &reg).unwrap(), once, "{name}");
    }
}

#[test]
fn format_normalizes_noncanonical_input() {
    let reg = registry();
    let formatted = format(&fixture("noncanonical.scene.ron"), &reg).unwrap();
    insta::assert_snapshot!(formatted);
    assert!(
        formatted.contains("translation: (5.0, 0.0, 0.0)"),
        "integers in float fields become floats"
    );
}

#[test]
fn insertion_order_does_not_change_output() {
    let reg = registry();
    let text = fixture("builtins.scene.ron");
    let mut doc = read_scene(&text, &reg).unwrap();
    doc.entities.reverse();
    assert_eq!(write_scene(&doc).unwrap(), text);
}

#[test]
fn float_formatting_is_shortest_roundtrip() {
    let points = vec![
        Vec3::new(0.1, 1e-7, -0.0),
        Vec3::new(f32::MAX, f32::MIN_POSITIVE, 1.0 / 3.0),
    ];
    let text = one_entity_scene(gadget(Mode::Patrol {
        points,
        looping: false,
    }));
    assert!(text.contains("(0.1, 1.0e-7, -0.0)"), "{text}");
    assert!(
        text.contains("(3.4028235e38, 1.1754944e-38, 0.33333334)"),
        "{text}"
    );
    let reg = registry();
    assert_eq!(
        write_scene(&read_scene(&text, &reg).unwrap()).unwrap(),
        text
    );
}

#[test]
fn non_finite_float_rejected_on_save() {
    let mut ids = DeterministicIdGen::new(1);
    let mut world = World::new();
    let id = world.spawn(&mut ids).unwrap();
    world.insert(id, Health { hp: f32::NAN }).unwrap();
    let meta = SceneMeta {
        id: ids.next_id().unwrap(),
        name: "S".into(),
    };
    let err = write_scene(&snapshot_world(&meta, &world).unwrap()).unwrap_err();
    assert_eq!(
        err,
        SceneError::NonFinite {
            entity: id,
            component: "Health".into(),
            pointer: "/hp".into()
        }
    );
}

#[test]
fn comments_and_header_are_regenerated() {
    let reg = registry();
    let formatted = format(&fixture("noncanonical.scene.ron"), &reg).unwrap();
    assert!(formatted.starts_with("// Concordia scene, format 1."));
    assert!(!formatted.contains("hand-edited") && !formatted.contains("trailing comment"));
}
