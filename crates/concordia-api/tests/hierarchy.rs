//! Entity templates and hierarchy editing for agents (M1 phase 1.13): `entity.templates`,
//! `entity.create`, `entity.duplicate`, `entity.reparent` and recursive `entity.despawn`.

mod common;

use std::sync::Arc;

use common::Harness;
use concordia_api::{EngineModule, EntityTemplate};
use serde_json::{Value, json};

#[derive(Debug)]
struct Props;

impl EngineModule for Props {
    fn name(&self) -> &'static str {
        "props"
    }

    fn templates(&self) -> Vec<EntityTemplate> {
        vec![EntityTemplate::new(
            "Marker",
            "A named point with a transform, for tests.",
            [
                ("Name", json!("Marker")),
                ("Transform", transform([0.0, 0.0, 0.0])),
            ],
        )]
    }
}

fn transform(t: [f64; 3]) -> Value {
    json!({"translation": t, "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]})
}

fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    Harness::with_config(dir, |c| c.modules = vec![Arc::new(Props)]).ready()
}

fn create(h: &mut Harness, params: Value) -> Value {
    h.ok("entity.create", params)["id"].clone()
}

fn component(h: &mut Harness, id: &Value, name: &str) -> Value {
    h.call("component.get", json!({"entity": id, "component": name}))
        .map(|v| v["value"].clone())
        .unwrap_or(Value::Null)
}

fn count(h: &mut Harness) -> u64 {
    h.ok("scene.info", json!({}))["entity_count"]
        .as_u64()
        .unwrap()
}

fn close(a: &Value, b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i].as_f64().unwrap() - b[i]).abs() < 1e-4)
}

#[test]
fn templates_come_from_the_core_and_modules_with_descriptions() {
    let mut h = harness();
    let list = h.ok("entity.templates", json!({}));
    let names: Vec<&str> = list["templates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"Empty") && names.contains(&"Marker"),
        "{list}"
    );
    for t in list["templates"].as_array().unwrap() {
        assert!(t["description"].as_str().unwrap().len() > 10, "{t}");
        assert!(t["components"].as_array().is_some(), "{t}");
    }
}

#[test]
fn entity_create_places_a_template_with_a_unique_name_parent_and_translation() {
    let mut h = harness();
    let root = create(&mut h, json!({"template": "Empty", "name": "Root"}));
    let a = create(
        &mut h,
        json!({"template": "Marker", "parent": root, "translation": [1.0, 2.0, 3.0]}),
    );
    let b = create(&mut h, json!({"template": "Marker", "parent": root}));
    assert_eq!(component(&mut h, &a, "Name"), "Marker");
    assert_eq!(
        component(&mut h, &b, "Name"),
        "Marker 2",
        "siblings get unique names"
    );
    assert_eq!(component(&mut h, &a, "Parent"), root);
    assert!(close(
        &component(&mut h, &a, "Transform")["translation"],
        [1.0, 2.0, 3.0]
    ));
    let err = h
        .call("entity.create", json!({"template": "Markr"}))
        .unwrap_err();
    assert_eq!(err.code(), -32602, "{err:?}");
    assert!(
        format!("{err:?}").contains("Marker"),
        "suggests the close match: {err:?}"
    );
    let err = h
        .call(
            "entity.create",
            json!({"template": "Marker", "parent": "0190f5c4-0000-7000-8000-000000000009"}),
        )
        .unwrap_err();
    assert!(err.to_string().contains("parent"), "{err:?}");
}

/// Root A with children B and C; B has child D. A refers to C in a component field.
fn tree(h: &mut Harness) -> (Value, Value, Value, Value) {
    let a = create(
        h,
        json!({"template": "Marker", "name": "A", "translation": [10.0, 0.0, 0.0]}),
    );
    let b = create(h, json!({"template": "Marker", "name": "B", "parent": a}));
    let c = create(h, json!({"template": "Marker", "name": "C", "parent": a}));
    let d = create(h, json!({"template": "Marker", "name": "D", "parent": b}));
    (a, b, c, d)
}

#[test]
fn entity_duplicate_copies_components_and_children_with_new_ids_in_one_undo() {
    let mut h = harness();
    let (a, b, _c, _d) = tree(&mut h);
    let before = count(&mut h);
    // Duplicating A and its descendant B copies A's subtree once.
    let dup = h.ok("entity.duplicate", json!({"ids": [a, b]}));
    let roots = dup["ids"].as_array().unwrap().clone();
    assert_eq!(roots.len(), 1, "{dup}");
    let copy = &roots[0];
    assert_ne!(copy, &a);
    assert_eq!(component(&mut h, copy, "Name"), "A 2");
    assert!(close(
        &component(&mut h, copy, "Transform")["translation"],
        [10.0, 0.0, 0.0]
    ));
    assert_eq!(count(&mut h), before + 4);
    let children = h.ok(
        "query.run",
        json!({"with": ["Parent"], "where": [{"path": "Parent", "op": "eq", "value": copy}]}),
    );
    let names: Vec<String> = children["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            component(&mut h, &r["id"], "Name")
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(names.len(), 2);
    assert!(
        names.contains(&"B".to_owned()) && names.contains(&"C".to_owned()),
        "{names:?}"
    );
    h.ok("txn.undo", json!({}));
    assert_eq!(count(&mut h), before, "one undo removes the whole copy");
}

#[test]
fn entity_reparent_keeps_the_world_transform_and_rejects_cycles() {
    let mut h = harness();
    let p = create(
        &mut h,
        json!({"template": "Marker", "name": "P", "translation": [10.0, 0.0, 0.0]}),
    );
    let c = create(
        &mut h,
        json!({"template": "Marker", "name": "C", "translation": [1.0, 0.0, 0.0]}),
    );
    h.ok("entity.reparent", json!({"id": c, "parent": p}));
    assert_eq!(component(&mut h, &c, "Parent"), p);
    assert!(
        close(
            &component(&mut h, &c, "Transform")["translation"],
            [-9.0, 0.0, 0.0]
        ),
        "the world position stays at x = 1"
    );
    let err = h
        .call("entity.reparent", json!({"id": p, "parent": c}))
        .unwrap_err();
    assert!(err.to_string().contains("cycle"), "{err:?}");
    let err = h
        .call("entity.reparent", json!({"id": p, "parent": p}))
        .unwrap_err();
    assert!(err.to_string().contains("cycle"), "{err:?}");
    h.ok("entity.reparent", json!({"id": c, "parent": null}));
    assert!(component(&mut h, &c, "Parent").is_null(), "unparented");
    assert!(close(
        &component(&mut h, &c, "Transform")["translation"],
        [1.0, 0.0, 0.0]
    ));
    h.ok(
        "entity.reparent",
        json!({"id": c, "parent": p, "keep_world": false}),
    );
    assert!(
        close(
            &component(&mut h, &c, "Transform")["translation"],
            [1.0, 0.0, 0.0]
        ),
        "keep_world false keeps the local transform"
    );
}

#[test]
fn despawn_recursive_removes_the_subtree_in_one_undo() {
    let mut h = harness();
    let (a, _b, _c, _d) = tree(&mut h);
    let before = count(&mut h);
    h.ok("entity.despawn", json!({"id": a, "recursive": true}));
    assert_eq!(count(&mut h), before - 4);
    h.ok("txn.undo", json!({}));
    assert_eq!(count(&mut h), before);
    h.ok("entity.despawn", json!({"id": a}));
    assert_eq!(
        count(&mut h),
        before - 1,
        "without recursive only the entity goes"
    );
}
