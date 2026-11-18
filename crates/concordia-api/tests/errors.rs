//! Agent-actionable errors (M0 phase 0.6).

mod common;

use common::Harness;
use concordia_api::{ApiError, ApiErrorKind};
use serde_json::json;

#[test]
fn unknown_component_suggests_closest_names() {
    let mut h = Harness::new().ready();
    let id = h.spawn("A", 0.0);
    let err = h.err(
        "component.set",
        json!({"entity": id, "component": "Transfrom", "value": {}}),
    );
    assert_eq!(err.code(), -32002);
    assert_eq!(err.data.kind, "ComponentNotRegistered");
    assert!(
        err.data.suggestions[0].contains("Transform"),
        "{:?}",
        err.data.suggestions
    );
    assert_eq!(err.data.see.as_deref(), Some("concordia://schema/index"));
}

#[test]
fn invalid_value_includes_pointer_expected_schema_and_example() {
    let mut h = Harness::new().ready();
    let id = h.spawn("A", 0.0);
    let value =
        json!({"translation": "far", "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]});
    let err = h.err(
        "component.set",
        json!({"entity": id, "component": "Transform", "value": value}),
    );
    assert_eq!(err.code(), -32003);
    assert_eq!(err.data.pointer.as_deref(), Some("/translation"));
    assert_eq!(err.data.expected.as_ref().unwrap()["maxItems"], 3);
    assert_eq!(
        err.data.example.as_ref().unwrap()["translation"],
        json!([0.0, 0.0, 0.0])
    );
    assert_eq!(
        err.data.see.as_deref(),
        Some("concordia://schema/component/Transform")
    );
}

#[test]
fn unknown_entity_suggests_query() {
    let mut h = Harness::new().ready();
    let player = h.spawn("Player", 0.0);
    let gone = h.spawn("Gone", 0.0);
    h.ok("entity.despawn", json!({"id": gone}));
    let ghost = "0190a000-0000-7000-8000-000000000000";
    let err = h.err("entity.get", json!({"id": ghost}));
    assert_eq!((err.code(), err.data.kind), (-32001, "EntityNotFound"));
    assert!(
        err.data.suggestions.iter().any(|s| s.contains("query.run")),
        "{:?}",
        err.data.suggestions
    );
    let err = h.err("entity.despawn", json!({"id": "Player"}));
    assert_eq!(err.data.kind, "EntityNotFound");
    assert!(
        err.data.suggestions.iter().any(|s| s.contains(&player)),
        "{:?}",
        err.data.suggestions
    );
}

#[test]
fn txn_conflict_names_holder() {
    let mut h = Harness::new().ready();
    h.ok("txn.begin", json!({"label": "mine"}));
    let other = h.engine.open_session();
    h.engine
        .call(
            other,
            "session.open",
            json!({"client": "b", "actor": "agent:b"}),
        )
        .unwrap();
    let err = h
        .engine
        .call(other, "entity.spawn", json!({"components": {}}))
        .unwrap_err();
    assert_eq!((err.code(), err.data.kind), (-32005, "TxnConflict"));
    let holder = &err.data.extra["holder"];
    assert_eq!(holder["actor"], "agent:test");
    assert_eq!(holder["label"], "mine");
    assert_eq!(holder["session"], h.session.to_string());
    assert!(holder["open_for_ms"].is_u64());
    assert!(
        err.data.suggestions.iter().any(|s| s.contains("retry")),
        "{:?}",
        err.data.suggestions
    );
}

#[test]
fn nothing_to_undo_explains_history_state() {
    let mut h = Harness::new().ready();
    let err = h.err("txn.undo", json!({}));
    assert_eq!((err.code(), err.data.kind), (-32006, "NothingToUndo"));
    assert!(err.message.contains("txn.history"), "{}", err.message);
}

#[test]
fn no_scene_open_suggests_scene_new() {
    let mut h = Harness::new();
    h.ok("session.open", json!({"client": "t", "actor": "agent:t"}));
    let err = h.err("entity.spawn", json!({"components": {}}));
    assert_eq!((err.code(), err.data.kind), (-32009, "NoSceneOpen"));
    assert!(err.data.suggestions.iter().any(|s| s.contains("scene.new")));
}

#[test]
fn every_error_kind_has_message_and_suggestion() {
    for kind in ApiErrorKind::ALL {
        let err = ApiError::from_kind(kind);
        assert!(!err.message.is_empty(), "{kind:?}");
        assert!(!err.data.suggestions.is_empty(), "{kind:?}");
        assert!(err.code() < 0, "{kind:?}");
        assert_eq!(err.data.kind, kind.name());
    }
}

#[test]
fn errors_serialize_as_jsonrpc_error_objects() {
    let err = ApiError::from_kind(ApiErrorKind::NotPlaying);
    let json = err.to_jsonrpc();
    assert_eq!(json["code"], -32011);
    assert_eq!(json["data"]["kind"], "NotPlaying");
    assert!(json["data"]["suggestions"].is_array());
}
