//! JSON Schema export (M0 phase 0.2).

mod common;

use common::{Health, Mode, Transform, zoo_registry};
use concordia_reflect::{Reflect, schema_for};

#[test]
fn schema_snapshot_health() {
    insta::assert_json_snapshot!(schema_for(Health::type_info()));
}

#[test]
fn schema_snapshot_transform() {
    insta::assert_json_snapshot!(schema_for(Transform::type_info()));
}

#[test]
fn schema_snapshot_enum_shape() {
    insta::assert_json_snapshot!(schema_for(Mode::type_info()));
}

#[test]
fn generated_schemas_validate_against_draft_2020_12_metaschema() {
    let reg = zoo_registry();
    for name in reg.names() {
        let schema = reg.schema_for(name).unwrap();
        if let Err(e) = jsonschema::draft202012::meta::validate(&schema) {
            panic!("{name}: {e}\n{schema:#}");
        }
    }
}

#[test]
fn registry_index_schema_lists_all_types_sorted() {
    let reg = zoo_registry();
    let index = reg.schema_index();
    let names: Vec<&str> = index["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
    assert_eq!(names, reg.names().collect::<Vec<_>>());
    insta::assert_json_snapshot!(index);
}

#[test]
fn unknown_name_has_no_schema() {
    assert!(zoo_registry().schema_for("Nope").is_none());
}

#[test]
fn non_zero_integers_have_minimum_one_and_reject_zero() {
    use std::num::{NonZeroU32, NonZeroU64};

    let schema = concordia_reflect::schema_for(NonZeroU32::type_info());
    assert_eq!(schema["type"], "integer");
    assert_eq!(schema["minimum"], 1);
    assert_eq!(schema["maximum"], u32::MAX);
    let err = concordia_reflect::validate_json(&serde_json::json!(0), NonZeroU32::type_info());
    assert!(err.is_err());
    assert!(
        concordia_reflect::validate_json(&serde_json::json!(5), NonZeroU64::type_info()).is_ok()
    );
    assert_eq!(
        concordia_reflect::example_json(NonZeroU32::type_info()),
        serde_json::json!(1)
    );
}
