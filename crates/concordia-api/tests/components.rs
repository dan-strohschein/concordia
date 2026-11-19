//! Component discovery (M1 phase 1.14): `component.add` inserts a type's default value.

mod common;

use common::Harness;
use concordia_api::ApiErrorKind;
use serde_json::json;

#[test]
fn component_add_inserts_the_default_and_rejects_duplicates_and_unknown_names() {
    let mut h = Harness::new().ready();
    let id = h.spawn("Box", 0.0);

    let added = h.ok(
        "component.add",
        json!({"entity": id, "component": "Velocity"}),
    );
    let zero = json!({"linear": [0.0, 0.0, 0.0], "angular": [0.0, 0.0, 0.0]});
    assert_eq!(added["value"], zero);
    let stored = h.ok(
        "component.get",
        json!({"entity": id, "component": "Velocity"}),
    );
    assert_eq!(stored["value"], zero);

    let duplicate = h.err(
        "component.add",
        json!({"entity": id, "component": "Velocity"}),
    );
    assert_eq!(duplicate.kind, ApiErrorKind::ValueInvalid);
    assert!(
        duplicate.message.contains("already has"),
        "{}",
        duplicate.message
    );
    assert!(
        duplicate
            .data
            .suggestions
            .iter()
            .any(|s| s.contains("component.patch")),
        "{:?}",
        duplicate.data.suggestions
    );

    let unknown = h.err(
        "component.add",
        json!({"entity": id, "component": "Velocty"}),
    );
    assert_eq!(unknown.kind, ApiErrorKind::ComponentNotRegistered);
    assert!(
        unknown
            .data
            .suggestions
            .iter()
            .any(|s| s.contains("\"Velocity\"")),
        "{:?}",
        unknown.data.suggestions
    );

    // Parent has no meaningful default: the error says how to add it instead.
    let no_default = h.err(
        "component.add",
        json!({"entity": id, "component": "Parent"}),
    );
    assert_eq!(no_default.kind, ApiErrorKind::ValueInvalid);
    assert!(
        no_default
            .data
            .suggestions
            .iter()
            .any(|s| s.contains("component.set")),
        "{:?}",
        no_default.data.suggestions
    );

    // One undo step removes the added component.
    h.ok("txn.undo", json!({}));
    let gone = h.err(
        "component.get",
        json!({"entity": id, "component": "Velocity"}),
    );
    assert_eq!(gone.kind, ApiErrorKind::ValueInvalid);
}

#[test]
fn schema_index_lists_core_components_with_category_and_addability() {
    let mut h = Harness::new().ready();
    let index = h.ok("schema.get", json!({}))["schema"].clone();
    let components = index["components"].as_array().expect("components list");
    let row = |name: &str| {
        components
            .iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing from {components:?}"))
            .clone()
    };
    assert_eq!(row("Transform")["category"], "Core");
    assert_eq!(row("Transform")["addable"], true);
    assert_eq!(row("Parent")["addable"], false);
    assert!(row("Velocity")["description"].as_str().unwrap().len() > 10);
    assert!(index["types"].as_array().unwrap().len() > components.len());
}
