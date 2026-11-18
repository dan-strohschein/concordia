//! Type registry and dynamic values (M0 phase 0.2).

mod common;

use common::{Health, Transform, zoo_registry};
use concordia_reflect::{DynReflect, Reflect, ReflectError, RegistryError, TypeRegistry};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Collides with `Health`'s stable name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Health")]
struct OtherHealth {
    /// Points.
    points: u32,
}

fn health_registry() -> TypeRegistry {
    let mut reg = TypeRegistry::new();
    reg.register::<Health>().unwrap();
    reg
}

#[test]
fn register_is_idempotent_and_registers_field_types() {
    let mut reg = TypeRegistry::new();
    reg.register::<Transform>().unwrap();
    let count = reg.names().count();
    reg.register::<Transform>().unwrap();
    assert_eq!(reg.names().count(), count);
    for name in ["Transform", "Vec3", "Quat", "f32"] {
        assert!(reg.get_by_name(name).is_some(), "{name} registered");
    }
}

#[test]
fn duplicate_stable_name_is_an_error() {
    let mut reg = health_registry();
    let err = reg.register::<OtherHealth>().unwrap_err();
    let RegistryError::NameCollision {
        name,
        existing,
        new,
    } = err;
    assert_eq!(name, "Health");
    assert!(existing.ends_with("Health") && new.ends_with("OtherHealth"));
}

#[test]
fn lookup_by_name_and_type_id() {
    let reg = health_registry();
    let by_name = reg.get_by_name("Health").unwrap();
    let by_type = reg.get::<Health>().unwrap();
    assert!(std::ptr::eq(by_name, by_type));
    assert_eq!(by_type.info().name, "Health");
    assert!(reg.get::<OtherHealth>().is_none());
}

#[derive(Debug, PartialEq)]
struct ReflectFoo(u32);

#[test]
fn type_data_slots_roundtrip() {
    let mut reg = health_registry();
    reg.get_mut::<Health>().unwrap().insert_data(ReflectFoo(7));
    let reg_entry = reg.get_by_name("Health").unwrap();
    assert_eq!(reg_entry.data::<ReflectFoo>(), Some(&ReflectFoo(7)));
    assert_eq!(reg_entry.data::<String>(), None);
}

#[test]
fn dyn_from_json_to_json_roundtrip() {
    let reg = health_registry();
    let input = json!({"hp": 50.0, "max": 100.0});
    let value = reg
        .get_by_name("Health")
        .unwrap()
        .from_json(input.clone())
        .unwrap();
    assert_eq!(value.to_json().unwrap(), input);
    let concrete = value.as_any().downcast_ref::<Health>().unwrap();
    assert_eq!(concrete.hp, 50.0);
}

#[test]
fn dyn_from_json_reports_json_pointer_on_error() {
    let reg = health_registry();
    let err = reg
        .get_by_name("Health")
        .unwrap()
        .from_json(json!({"hp": "lots", "max": 1.0}))
        .unwrap_err();
    assert_eq!(
        err,
        ReflectError::Invalid {
            pointer: "/hp".into(),
            expected: "number".into(),
            found: "string".into(),
        }
    );
}

#[test]
fn dyn_from_json_reports_missing_unknown_and_out_of_range() {
    let reg = zoo_registry();
    let health = reg.get_by_name("Health").unwrap();
    assert!(matches!(
        health.from_json(json!({"hp": 1.0})),
        Err(ReflectError::MissingField { pointer, field }) if pointer.is_empty() && field == "max"
    ));
    assert!(matches!(
        health.from_json(json!({"hp": 1.0, "max": 2.0, "mana": 3})),
        Err(ReflectError::UnknownField { field, .. }) if field == "mana"
    ));
    assert!(matches!(
        health.from_json(json!({"hp": 5000.0, "max": 2.0})),
        Err(ReflectError::OutOfRange { pointer, .. }) if pointer == "/hp"
    ));
    let transform = reg.get_by_name("Transform").unwrap();
    assert!(matches!(
        transform.from_json(json!({"translation": [0, 0], "rotation": [0, 0, 0, 1], "scale": [1, 1, 1]})),
        Err(ReflectError::Invalid { pointer, .. }) if pointer == "/translation"
    ));
}

#[test]
fn from_json_rejects_unknown_type_name() {
    let reg = health_registry();
    assert_eq!(
        reg.from_json("Healht", json!({})).unwrap_err(),
        ReflectError::UnknownType {
            name: "Healht".into()
        }
    );
    assert!(
        reg.from_json("Health", json!({"hp": 1.0, "max": 2.0}))
            .is_ok()
    );
}

#[test]
fn erased_deserialize_builds_value() {
    let reg = health_registry();
    let mut de = serde_json::Deserializer::from_str(r#"{"hp": 3.0, "max": 4.0}"#);
    let mut erased = <dyn erased_serde::Deserializer>::erase(&mut de);
    let value = (reg.get_by_name("Health").unwrap().deserialize)(&mut erased).unwrap();
    assert_eq!(
        value.as_any().downcast_ref::<Health>(),
        Some(&Health { hp: 3.0, max: 4.0 })
    );
}

#[test]
fn dyn_clone_and_eq() {
    let a: Box<dyn DynReflect> = Box::new(Health { hp: 1.0, max: 2.0 });
    let b = a.clone_dyn();
    assert!(a.eq_dyn(b.as_ref()));
    let c: Box<dyn DynReflect> = Box::new(Health { hp: 9.0, max: 2.0 });
    assert!(!a.eq_dyn(c.as_ref()));
    assert_eq!(a.info().name, "Health");
}
