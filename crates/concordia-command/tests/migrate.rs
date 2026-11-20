//! Migrating a script type in the edit world (M2 phase 2.6d, ADR-0061): one attributed,
//! journaled transaction that undoes to the exact previous type version and values, redoes,
//! and replays.

mod common;

use std::collections::BTreeMap;

use common::{Fixture, author};
use concordia_command::{CommandError, EditOp, Journal, JournalConfig, replay_journal};
use concordia_core::id::EntityId;
use concordia_reflect::{TypeDesc, TypeInfo, TypeRegistry};
use concordia_terra::register_dynamic_component;
use serde_json::{Value, json};

fn hp(fields: Value) -> TypeDesc {
    serde_json::from_value(json!({
        "name": "s.Hp", "docs": "Health.",
        "shape": {"Struct": {"fields": fields, "deny_unknown": true}},
    }))
    .unwrap()
}

fn field(name: &str, ty: &str) -> Value {
    json!({"name": name, "ty": {"Named": ty}, "docs": name})
}

/// Version 1: `value`, `note`. Version 2: `value`, `max` (default 100). Text: `value` a string.
fn versions() -> (TypeDesc, TypeDesc, TypeDesc) {
    let v1 = hp(json!([field("value", "u32"), field("note", "String")]));
    let v2 = hp(json!([
        field("value", "u32"),
        {"name": "max", "ty": {"Named": "u32"}, "docs": "max", "default": 100}
    ]));
    let text = hp(json!([field("value", "String"), field("note", "String")]));
    (v1, v2, text)
}

fn current(reg: &TypeRegistry) -> &'static TypeInfo {
    reg.get_by_name("s.Hp").unwrap().info()
}

/// Two entities with `s.Hp` version 1.
fn fixture() -> (Fixture, Vec<EntityId>) {
    let mut fx = Fixture::new();
    let (v1, _, _) = versions();
    register_dynamic_component(&mut fx.reg, &v1).unwrap();
    let ids: Vec<EntityId> = (0..2).map(|_| fx.entity_id()).collect();
    for (i, &id) in ids.iter().enumerate() {
        let components = BTreeMap::from([("s.Hp".to_owned(), json!({"value": i, "note": "n"}))]);
        fx.one("human:dev", EditOp::Spawn { id, components });
    }
    (fx, ids)
}

fn migrate_to(desc: &TypeDesc) -> EditOp {
    EditOp::MigrateType {
        component: "s.Hp".into(),
        to: serde_json::to_value(desc).unwrap(),
    }
}

#[test]
fn migrate_type_is_one_attributed_transaction_that_undoes_exactly() {
    let (mut fx, ids) = fixture();
    let (_, v2, _) = versions();
    let v1_info = current(&fx.reg);
    let before = fx.hash();
    fx.one("script:r", migrate_to(&v2));
    let v2_info = current(&fx.reg);
    assert!(
        !std::ptr::eq(v1_info, v2_info),
        "the registry holds the new version"
    );
    assert_eq!(
        fx.world.get_json(ids[1], "s.Hp").unwrap(),
        json!({"value": 1, "max": 100})
    );
    let history = fx.log.history(1);
    assert_eq!(history[0].actor.to_string(), "script:r");
    let after = fx.hash();

    fx.log
        .undo(&mut fx.world, &mut fx.reg, author("human:dev"))
        .unwrap();
    assert!(
        std::ptr::eq(current(&fx.reg), v1_info),
        "undo restores the exact version"
    );
    assert_eq!(
        fx.world.get_json(ids[1], "s.Hp").unwrap(),
        json!({"value": 1, "note": "n"})
    );
    assert_eq!(
        fx.hash(),
        before,
        "and the exact values, removed fields included"
    );

    fx.log
        .redo(&mut fx.world, &mut fx.reg, author("human:dev"))
        .unwrap();
    assert!(
        std::ptr::eq(current(&fx.reg), v2_info),
        "redo returns to the same new version"
    );
    assert_eq!(fx.hash(), after);
}

#[test]
fn a_journal_with_a_migration_replays_from_its_base() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.cmdlog.jsonl");
    let mut fx = Fixture::new();
    let (v1, v2, _) = versions();
    register_dynamic_component(&mut fx.reg, &v1).unwrap();
    let base = fx.world.clone();
    let (journal, _) = Journal::open(&path, JournalConfig::default()).unwrap();
    fx.log.attach_journal(journal);
    let id = fx.entity_id();
    let components = BTreeMap::from([("s.Hp".to_owned(), json!({"value": 3, "note": "n"}))]);
    fx.one("human:dev", EditOp::Spawn { id, components });
    fx.one("script:r", migrate_to(&v2));
    fx.one(
        "human:dev",
        EditOp::SetComponent {
            id,
            component: "s.Hp".into(),
            value: json!({"value": 4, "max": 7}),
        },
    );
    let entries = Journal::load(&path).unwrap().entries;

    let mut reg = Fixture::new().reg;
    register_dynamic_component(&mut reg, &v1).unwrap();
    let mut world = base;
    let replayed = replay_journal(&entries, &mut world, &mut reg).unwrap();
    assert_eq!(replayed, Some(fx.hash()));
    assert_eq!(
        world.get_json(id, "s.Hp").unwrap(),
        json!({"value": 4, "max": 7})
    );
}

#[test]
fn a_migration_that_cannot_convert_changes_nothing() {
    let (mut fx, ids) = fixture();
    let (_, _, text) = versions();
    let v1_info = current(&fx.reg);
    let before = (fx.hash(), fx.log.history(10).len());
    let err = fx
        .log
        .apply_one(
            &mut fx.world,
            &mut fx.reg,
            author("script:r"),
            "migrate",
            migrate_to(&text),
            &mut fx.ids,
        )
        .unwrap_err();
    assert!(matches!(err, CommandError::OpFailed { .. }), "{err}");
    assert!(
        std::ptr::eq(current(&fx.reg), v1_info),
        "the registry is unchanged"
    );
    assert_eq!((fx.hash(), fx.log.history(10).len()), before);
    fx.world
        .set_json(
            ids[0],
            "s.Hp",
            json!({"value": 9, "note": "still v1"}),
            &fx.reg,
        )
        .unwrap();
}

#[test]
fn migrate_type_refuses_rust_types_and_bad_descriptions() {
    let (mut fx, _) = fixture();
    let mut apply = |op: EditOp| {
        fx.log
            .apply_one(
                &mut fx.world,
                &mut fx.reg,
                author("script:r"),
                "m",
                op,
                &mut fx.ids,
            )
            .unwrap_err()
            .to_string()
    };
    let (_, v2, _) = versions();
    let mut pos = v2.clone();
    pos.name = "Pos".into();
    assert!(
        apply(EditOp::MigrateType {
            component: "Pos".into(),
            to: serde_json::to_value(&pos).unwrap()
        })
        .contains("Pos")
    );
    assert!(
        apply(EditOp::MigrateType {
            component: "s.Hp".into(),
            to: json!({"not": "a description"})
        })
        .contains("description")
    );
    assert!(
        apply(EditOp::MigrateType {
            component: "s.Other".into(),
            to: serde_json::to_value(&v2).unwrap()
        })
        .contains("s.Other")
    );
}
