//! Proves generated schemas agree with serde's JSON (M0 phase 0.2).

mod common;

use std::collections::BTreeMap;

use common::{Health, Marker, Mode, Name, Pair, Strict, Transform, Zoo};
use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
use concordia_core::math::{Quat, Vec2, Vec3};
use concordia_reflect::{Reflect, schema_for};
use proptest::prelude::*;
use serde_json::{Value, json};

fn validator<T: Reflect>() -> jsonschema::Validator {
    jsonschema::draft202012::new(&schema_for(T::type_info())).unwrap()
}

fn finite32() -> impl Strategy<Value = f32> {
    any::<f32>().prop_filter("finite", |x| x.is_finite())
}

fn finite64() -> impl Strategy<Value = f64> {
    any::<f64>().prop_filter("finite", |x| x.is_finite())
}

fn vec3() -> impl Strategy<Value = Vec3> {
    (finite32(), finite32(), finite32()).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

fn mode() -> impl Strategy<Value = Mode> {
    prop_oneof![
        Just(Mode::Idle),
        finite32().prop_map(Mode::Walk),
        (finite32(), finite32()).prop_map(|(h, d)| Mode::Jump(h, d)),
        (prop::collection::vec(vec3(), 0..4), any::<bool>())
            .prop_map(|(points, looping)| Mode::Patrol { points, looping }),
    ]
}

fn entity_id() -> impl Strategy<Value = EntityId> {
    any::<u64>().prop_map(|seed| DeterministicIdGen::new(seed).next_id().unwrap())
}

prop_compose! {
    fn zoo()(
        name in ".*",
        // `Health.hp` declares range(0..=1000); serde does not enforce it, the schema does.
        hp in 0.0f32..=1000.0, max in finite32(),
        mode in mode(),
        tags in prop::collection::vec(".*", 0..4),
        stats in prop::collection::btree_map(".*", finite64(), 0..4),
        target in prop::option::of(entity_id()),
        offset in prop::option::of((finite32(), finite32()).prop_map(|(x, y)| Vec2::new(x, y))),
        pair in (any::<i32>(), ".*"),
        grid in any::<[u8; 3]>(),
        tuple in any::<(i32, bool)>(),
    ) -> Zoo {
        Zoo {
            name: Name(name), health: Health { hp, max }, mode, tags,
            stats: stats.into_iter().collect::<BTreeMap<_, _>>(), target, offset,
            pair: Pair(pair.0, pair.1), marker: Marker, grid, tuple, cache: 0,
        }
    }
}

proptest! {
    #[test]
    fn serialized_values_validate_against_their_schema(value in zoo()) {
        let json = serde_json::to_value(&value).unwrap();
        let validator = validator::<Zoo>();
        prop_assert!(validator.is_valid(&json), "{json:#}");
    }

    #[test]
    fn transforms_validate_against_their_schema(t in vec3(), s in vec3()) {
        let value = Transform { translation: t, rotation: Quat::IDENTITY, scale: s };
        prop_assert!(validator::<Transform>().is_valid(&serde_json::to_value(&value).unwrap()));
    }
}

fn assert_invalid<T: Reflect>(instance: Value) {
    assert!(
        !validator::<T>().is_valid(&instance),
        "should reject {instance}"
    );
}

#[test]
fn schema_rejects_wrong_shapes() {
    assert_invalid::<Health>(json!({"hp": 1.0}));
    assert_invalid::<Health>(json!({"hp": "x", "max": 1.0}));
    assert_invalid::<Mode>(json!("Run"));
    assert_invalid::<Mode>(json!({"Walk": [1.0]}));
    assert_invalid::<Mode>(json!({"Patrol": {"points": []}}));
    assert_invalid::<Strict>(json!({"a": 1, "b": 2}));
    assert_invalid::<Strict>(json!({"a": 300}));
    assert_invalid::<Transform>(
        json!({"translation": [0, 0], "rotation": [0, 0, 0, 1], "scale": [1, 1, 1]}),
    );
    assert_invalid::<Pair>(json!([1]));
}
