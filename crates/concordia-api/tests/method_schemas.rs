//! Method registry and generated schemas (M0 phase 0.6).

use concordia_api::{methods, params_schema, result_schema};
use serde_json::json;

#[test]
fn registry_has_the_25_m0_methods_and_the_m1_core_methods() {
    assert_eq!(
        methods().iter().count(),
        33,
        "25 from M0, events.subscribe/unsubscribe (M1 phase 1.9), entity.templates/create/duplicate/reparent (M1 phase 1.13), component.add (M1 phase 1.14) and txn.undo_to (M1 phase 1.15)"
    );
    for name in [
        "session.open",
        "scene.new",
        "entity.spawn",
        "component.patch",
        "query.run",
        "txn.history",
        "sim.step",
        "log.tail",
        "events.subscribe",
        "events.unsubscribe",
        "component.add",
        "txn.undo_to",
    ] {
        assert!(methods().get(name).is_some(), "{name}");
    }
}

#[test]
fn every_method_has_generated_param_and_result_schema() {
    for method in methods().iter() {
        assert!(
            !method.docs.trim().is_empty(),
            "{} has no description",
            method.name
        );
        for schema in [params_schema(method), result_schema(method)] {
            assert_eq!(schema["type"], "object", "{}", method.name);
            if let Err(e) = jsonschema::draft202012::meta::validate(&schema) {
                panic!("{}: {e}", method.name);
            }
        }
    }
}

#[test]
fn method_index_snapshot() {
    let index: Vec<_> = methods()
        .iter()
        .map(|m| json!({ "name": m.name, "mutates": m.mutates, "description": m.docs.lines().next().unwrap_or("") }))
        .collect();
    insta::assert_json_snapshot!(index);
}

#[test]
fn mutating_methods_accept_actor_field() {
    for method in methods().iter().filter(|m| m.mutates) {
        let schema = params_schema(method);
        let actor = &schema["properties"]["actor"];
        let pattern = actor["pattern"]
            .as_str()
            .or_else(|| actor["anyOf"][1]["pattern"].as_str());
        assert_eq!(
            pattern,
            Some(concordia_command::ACTOR_PATTERN),
            "{}: {actor}",
            method.name
        );
        assert!(
            !schema["required"]
                .as_array()
                .is_some_and(|r| r.contains(&json!("actor"))),
            "{} actor is optional",
            method.name
        );
    }
}
