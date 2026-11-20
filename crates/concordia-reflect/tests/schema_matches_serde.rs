//! Proves generated schemas agree with serde's JSON (M0 phase 0.2).

mod common;

use common::arb::{vec3, zoo};
use common::{Health, Mode, Pair, Strict, Transform, Zoo};
use concordia_core::math::Quat;
use concordia_reflect::{Reflect, schema_for};
use proptest::prelude::*;
use serde_json::{Value, json};

fn validator<T: Reflect>() -> jsonschema::Validator {
    jsonschema::draft202012::new(&schema_for(T::type_info())).unwrap()
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
