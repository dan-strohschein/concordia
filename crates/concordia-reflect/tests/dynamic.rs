//! Dynamic types (M2 phase 2.6a, ADR-0062): types described at runtime (by script modules) that
//! behave exactly like `#[derive(Reflect)]` types of the same shape.

mod common;

use std::collections::BTreeMap;

use common::{Health, Marker, Mode, Name, Pair, Zoo, zoo_registry};
use concordia_core::id::{DeterministicIdGen, IdSource};
use concordia_core::math::{Vec2, Vec3};
use concordia_reflect::{
    DynReflect, DynamicValue, Reflect, ReflectError, RegistryError, TypeDesc, TypeInfo,
    TypeRegistry, canonical_hash, migrate, schema_for,
};
use serde_json::{Value, json};

/// Registers a dynamic type with `T`'s shape under `name`.
fn dynamic_twin<T: Reflect>(reg: &mut TypeRegistry, name: &str) -> &'static TypeInfo {
    let mut desc = T::type_info().to_desc();
    desc.name = name.to_owned();
    reg.register_dynamic(&desc).expect("register twin").info()
}

fn digest(value: &dyn DynReflect) -> String {
    let mut hasher = blake3::Hasher::new();
    canonical_hash(value, &mut hasher).unwrap();
    hasher.finalize().to_hex().to_string()
}

/// A `Zoo` that exercises every shape with non-empty values.
fn rich_zoo() -> Zoo {
    Zoo {
        name: Name("zebra".into()),
        health: Health {
            hp: 12.5,
            max: 40.0,
        },
        mode: Mode::Patrol {
            points: vec![Vec3::new(1.0, 2.0, 3.0), Vec3::new(-4.0, 0.5, 9.0)],
            looping: true,
        },
        tags: vec!["fast".into(), "striped".into()],
        stats: BTreeMap::from([("speed".into(), 3.25), ("armor".into(), -1.0)]),
        target: Some(DeterministicIdGen::new(7).next_id().unwrap()),
        offset: Some(Vec2::new(0.5, -0.25)),
        pair: Pair(-3, "three".into()),
        marker: Marker,
        grid: [1, 2, 3],
        tuple: (-9, true),
        cache: 0,
    }
}

/// A struct description built the way a script module sends it.
fn brain_desc(fields: Value) -> TypeDesc {
    serde_json::from_value(json!({
        "name": "Brain",
        "docs": "What an enemy is thinking.",
        "shape": {"Struct": {"fields": fields, "deny_unknown": true}},
    }))
    .expect("valid TypeDesc json")
}

fn field(name: &str, ty: Value, extra: Value) -> Value {
    let mut f = json!({"name": name, "ty": ty, "docs": format!("The {name}.")});
    if let (Some(f), Some(extra)) = (f.as_object_mut(), extra.as_object()) {
        f.extend(extra.clone());
    }
    f
}

fn named(name: &str) -> Value {
    json!({"Named": name})
}

fn option_of(name: &str) -> Value {
    json!({"Structural": {"Option": {"Named": name}}})
}

#[test]
fn type_desc_roundtrips_type_info() {
    let reg = zoo_registry();
    let desc = Zoo::type_info().to_desc();
    let wire = serde_json::to_value(&desc).unwrap();
    let back: TypeDesc = serde_json::from_value(wire).unwrap();
    assert_eq!(back, desc);
    let info = back
        .to_info(&reg)
        .expect("every referenced type is registered");
    assert_eq!(
        info.to_desc(),
        desc,
        "docs, defaults, ranges and shapes survive"
    );
    assert_eq!(schema_for(info), schema_for(Zoo::type_info()));
}

#[test]
fn dynamic_value_serializes_like_the_equivalent_static_type() {
    let mut reg = zoo_registry();
    let twin = dynamic_twin::<Zoo>(&mut reg, "ZooTwin");
    let sample = rich_zoo();
    let json = serde_json::to_value(&sample).unwrap();
    let dynamic = DynamicValue::from_json(twin, json.clone()).unwrap();
    assert_eq!(dynamic.info().name, "ZooTwin");
    let as_dyn: &dyn DynReflect = &dynamic;
    assert_eq!(as_dyn.to_json().unwrap(), json);
    assert_eq!(digest(&dynamic), digest(&sample), "canonical hash");
    assert!(as_dyn.eq_dyn(&dynamic.clone()));
    let other = DynamicValue::from_json(twin, serde_json::to_value(common::zoo_sample()).unwrap());
    assert!(!as_dyn.eq_dyn(&other.unwrap()));
}

#[test]
fn dynamic_type_registers_by_name_with_schema_and_validation() {
    let mut reg = zoo_registry();
    dynamic_twin::<Health>(&mut reg, "Vitality");
    let registration = reg.get_by_name("Vitality").expect("registered by name");
    assert!(registration.is_dynamic());
    assert!(!reg.get_by_name("Health").unwrap().is_dynamic());
    assert_eq!(registration.info().docs, "A health pool.");
    assert_eq!(
        reg.schema_for("Vitality").unwrap()["properties"],
        reg.schema_for("Health").unwrap()["properties"]
    );
    assert!(reg.names().any(|n| n == "Vitality"));

    let value = reg
        .from_json("Vitality", json!({"hp": 5.0, "max": 10.0}))
        .unwrap();
    assert_eq!(value.info().name, "Vitality");
    assert!(value.as_any().downcast_ref::<DynamicValue>().is_some());
    assert_eq!(value.to_json().unwrap(), json!({"hp": 5.0, "max": 10.0}));
    assert!(matches!(
        reg.from_json("Vitality", json!({"hp": 5000.0, "max": 1.0})),
        Err(ReflectError::OutOfRange { .. })
    ));
    assert!(matches!(
        reg.from_json("Vitality", json!({"hp": "x", "max": 1.0})),
        Err(ReflectError::Invalid { .. })
    ));

    // Scene loading builds values through an erased deserializer.
    let mut de = serde_json::Deserializer::from_str(r#"{"hp": 1.0, "max": 2.0}"#);
    let mut erased = <dyn erased_serde::Deserializer>::erase(&mut de);
    let loaded = registration.deserialize(&mut erased).unwrap();
    assert_eq!(loaded.to_json().unwrap(), json!({"hp": 1.0, "max": 2.0}));
}

#[test]
fn dynamic_type_name_collision_is_an_error() {
    let mut reg = zoo_registry();
    let mut desc = Health::type_info().to_desc();
    assert!(matches!(
        reg.register_dynamic(&desc),
        Err(RegistryError::NameCollision { name: "Health", .. })
    ));

    desc.name = String::new();
    assert!(matches!(
        reg.register_dynamic(&desc),
        Err(RegistryError::InvalidDesc { .. })
    ));

    let unknown = brain_desc(json!([field("x", named("Nope"), json!({}))]));
    assert!(matches!(
        reg.register_dynamic(&unknown),
        Err(RegistryError::UnknownType { name, .. }) if name == "Nope"
    ));

    let duplicate = brain_desc(json!([
        field("x", named("f32"), json!({})),
        field("x", named("f32"), json!({})),
    ]));
    assert!(matches!(
        reg.register_dynamic(&duplicate),
        Err(RegistryError::InvalidDesc { .. })
    ));
    assert!(
        reg.get_by_name("Brain").is_none(),
        "failed registrations leave nothing behind"
    );
}

#[test]
fn reregistering_a_dynamic_type_replaces_its_schema() {
    let mut reg = zoo_registry();
    let v1 = brain_desc(json!([field("mood", named("f32"), json!({}))]));
    let v2 = brain_desc(json!([
        field("mood", named("f32"), json!({})),
        field("courage", named("f32"), json!({"default": 0.5})),
    ]));
    reg.register_dynamic(&v1).unwrap();
    reg.register_dynamic(&v2).unwrap();
    let info = reg.get_by_name("Brain").unwrap().info();
    assert_eq!(info.to_desc(), v2.to_info(&reg).unwrap().to_desc());
    assert_eq!(reg.names().filter(|n| *n == "Brain").count(), 1);
    assert!(reg.from_json("Brain", json!({"mood": 1.0})).is_err());
    assert!(
        reg.from_json("Brain", json!({"mood": 1.0, "courage": 2.0}))
            .is_ok()
    );
}

#[test]
fn migration_fills_added_fields_with_defaults_and_reports_removed_ones() {
    let mut reg = zoo_registry();
    let v1 = reg
        .register_dynamic(&brain_desc(json!([
            field("mood", named("f32"), json!({})),
            field("target", option_of("EntityId"), json!({"optional": true})),
            field("notes", named("String"), json!({})),
        ])))
        .unwrap()
        .info();
    let old =
        DynamicValue::from_json(v1, json!({"mood": 0.25, "target": null, "notes": "x"})).unwrap();

    let v2 = reg
        .register_dynamic(&brain_desc(json!([
            field("mood", named("f32"), json!({})),
            field("courage", named("f32"), json!({"default": 0.5})),
            field("target", option_of("EntityId"), json!({"optional": true})),
            field("alert", option_of("bool"), json!({"optional": true})),
        ])))
        .unwrap()
        .info();
    let (new, report) = migrate(&old, v2).unwrap();
    assert_eq!(new.info().name, "Brain");
    let as_dyn: &dyn DynReflect = &new;
    assert_eq!(
        as_dyn.to_json().unwrap(),
        json!({"mood": 0.25, "courage": 0.5, "target": null, "alert": null})
    );
    assert_eq!(
        report.added,
        vec!["/courage".to_owned(), "/alert".to_owned()]
    );
    assert_eq!(report.removed, vec!["/notes".to_owned()]);

    let retyped = reg
        .register_dynamic(&brain_desc(json!([field(
            "mood",
            named("String"),
            json!({})
        )])))
        .unwrap()
        .info();
    assert!(matches!(
        migrate(&new, retyped),
        Err(ReflectError::Invalid { pointer, .. }) if pointer == "/mood"
    ));

    let required = reg
        .register_dynamic(&brain_desc(json!([
            field("mood", named("f32"), json!({})),
            field("rage", named("f32"), json!({})),
        ])))
        .unwrap()
        .info();
    assert!(matches!(
        migrate(&new, required),
        Err(ReflectError::MissingField { field, .. }) if field == "rage"
    ));
}
