//! Reflection features the Engine API needs (M0 phase 0.6).

mod common;

use std::sync::OnceLock;

use common::{Health, zoo_registry};
use concordia_reflect::{
    Primitive, Reflect, ReflectError, Shape, TypeInfo, example_json, schema_for, validate_json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A lowercase word.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Word(String);

impl Reflect for Word {
    fn type_info() -> &'static TypeInfo {
        static INFO: OnceLock<TypeInfo> = OnceLock::new();
        INFO.get_or_init(|| {
            TypeInfo::named(
                "Word",
                "Word",
                "A lowercase word.",
                Shape::Primitive(Primitive::Pattern("^[a-z]+$")),
            )
        })
    }
}

/// Uses a keyword as a field name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
struct Query {
    /// Filter.
    r#where: u8,
}

#[test]
fn validate_json_reports_pointer() {
    let err = validate_json(&json!({"hp": "x", "max": 1.0}), Health::type_info()).unwrap_err();
    assert!(matches!(err, ReflectError::Invalid { pointer, .. } if pointer == "/hp"));
    assert!(validate_json(&json!({"hp": 1.0, "max": 1.0}), Health::type_info()).is_ok());
}

#[test]
fn pattern_primitive_has_schema_pattern_and_validates_as_string() {
    let schema = schema_for(Word::type_info());
    assert_eq!(schema["type"], "string");
    assert_eq!(schema["pattern"], "^[a-z]+$");
    assert!(validate_json(&json!("abc"), Word::type_info()).is_ok());
    assert!(validate_json(&json!(3), Word::type_info()).is_err());
}

#[test]
fn raw_identifier_field_uses_plain_name() {
    let Shape::Struct { fields, .. } = &Query::type_info().shape else {
        panic!()
    };
    assert_eq!(fields[0].name, "where");
    assert_eq!(
        serde_json::to_value(Query { r#where: 1 }).unwrap(),
        json!({"where": 1})
    );
}

#[test]
fn example_json_uses_field_defaults() {
    assert_eq!(
        example_json(Health::type_info()),
        json!({"hp": 100.0, "max": 0.0})
    );
}

#[test]
fn example_json_validates_against_its_schema() {
    let reg = zoo_registry();
    for name in reg.names() {
        let info = reg.get_by_name(name).unwrap().info();
        let example = example_json(info);
        let validator = jsonschema::draft202012::new(&schema_for(info)).unwrap();
        assert!(validator.is_valid(&example), "{name}: {example}");
        assert!(validate_json(&example, info).is_ok(), "{name}: {example}");
    }
}
