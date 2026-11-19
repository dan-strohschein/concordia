//! JSON-RPC 2.0 dispatch and method behavior (M0 phase 0.6).

mod common;

use common::Harness;
use serde_json::{Value, json};

fn rpc(h: &mut Harness, text: &str) -> Option<Value> {
    h.engine
        .handle_jsonrpc(h.session, text)
        .map(|s| serde_json::from_str(&s).unwrap())
}

#[test]
fn jsonrpc_request_response_roundtrip() {
    let mut h = Harness::new().ready();
    let response = rpc(
        &mut h,
        r#"{"jsonrpc":"2.0","id":1,"method":"scene.info","params":{}}"#,
    )
    .unwrap();
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["name"], "Test");
    let response = rpc(
        &mut h,
        r#"{"jsonrpc":"2.0","id":"a","method":"scene.info"}"#,
    )
    .unwrap();
    assert_eq!(response["id"], "a", "params may be omitted");
}

#[test]
fn jsonrpc_batch_and_notifications() {
    let mut h = Harness::new().ready();
    assert_eq!(
        rpc(&mut h, r#"{"jsonrpc":"2.0","method":"scene.info"}"#),
        None
    );
    let batch = rpc(
        &mut h,
        r#"[{"jsonrpc":"2.0","id":1,"method":"scene.info"},{"jsonrpc":"2.0","method":"scene.info"},{"jsonrpc":"2.0","id":2,"method":"nope"}]"#,
    )
    .unwrap();
    let ids: Vec<_> = batch
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].clone())
        .collect();
    assert_eq!(ids, vec![json!(1), json!(2)]);
    assert_eq!(
        rpc(&mut h, r#"[{"jsonrpc":"2.0","method":"scene.info"}]"#),
        None
    );
}

#[test]
fn parse_error_and_invalid_request_codes() {
    let mut h = Harness::new().ready();
    let code = |v: Option<Value>| v.unwrap()["error"]["code"].clone();
    assert_eq!(code(rpc(&mut h, "{not json")), -32700);
    assert_eq!(code(rpc(&mut h, "[]")), -32600);
    assert_eq!(
        code(rpc(
            &mut h,
            r#"{"jsonrpc":"1.0","id":1,"method":"scene.info"}"#
        )),
        -32600
    );
    let missing = rpc(&mut h, r#"{"jsonrpc":"2.0","id":1,"method":"scene.inf"}"#).unwrap();
    assert_eq!(missing["error"]["code"], -32601);
    assert!(
        missing["error"]["data"]["suggestions"]
            .to_string()
            .contains("scene.info"),
        "{missing}"
    );
    let bad = rpc(
        &mut h,
        r#"{"jsonrpc":"2.0","id":1,"method":"sim.step","params":{"ticks":"many"}}"#,
    )
    .unwrap();
    assert_eq!(bad["error"]["code"], -32602);
    assert_eq!(bad["error"]["data"]["pointer"], "/ticks");
}

#[test]
fn spawn_set_get_roundtrip_via_dispatch() {
    let mut h = Harness::new().ready();
    let id = h.spawn("Player", 0.0);
    let value = json!({"translation": [5.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]});
    let set = h.ok(
        "component.set",
        json!({"entity": id, "component": "Transform", "value": value}),
    );
    assert_eq!(set["previous"]["translation"], json!([0.0, 0.0, 0.0]));
    assert_eq!(
        h.ok(
            "component.get",
            json!({"entity": id, "component": "Transform"})
        )["value"],
        value
    );
    let entity = h.ok("entity.get", json!({"id": id}));
    assert_eq!(entity["components"]["Name"], "Player");
    let removed = h.ok(
        "component.remove",
        json!({"entity": id, "component": "Name"}),
    );
    assert_eq!(removed["previous"], "Player");
    h.ok("entity.despawn", json!({"id": id}));
    assert_eq!(h.ok("scene.info", json!({}))["entity_count"], 0);
}

#[test]
fn mutation_without_actor_and_without_session_default_fails() {
    let mut h = Harness::new();
    let err = h.err(
        "scene.new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    );
    assert_eq!(err.code(), -32004);
    assert_eq!(err.data.kind, "ActorRequired");
    assert_eq!(
        err.data.suggestions[0],
        "pass \"actor\": \"agent:<role>\" or call session.open with an actor"
    );
    let err = h.err(
        "scene.new",
        json!({"path": "scenes/a.scene.ron", "name": "A", "actor": "claude"}),
    );
    assert_eq!(err.data.kind, "ActorMalformed");
    assert!(
        err.data.suggestions[0].contains("agent:claude-code"),
        "{err:?}"
    );
}

#[test]
fn mutation_records_session_default_actor() {
    let mut h = Harness::new().ready();
    h.spawn("A", 0.0);
    h.ok(
        "entity.spawn",
        json!({"components": {}, "actor": "agent:other#2"}),
    );
    let entries = h.ok("txn.history", json!({}))["entries"].clone();
    let actors: Vec<_> = entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["actor"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actors, vec!["agent:test", "agent:other#2"]);
}

#[test]
fn query_run_filters_and_sorts() {
    let mut h = Harness::new().ready();
    let ids: Vec<String> = (0..5)
        .map(|i| h.spawn(&format!("e{i}"), i as f32))
        .collect();
    h.ok(
        "entity.spawn",
        json!({"components": {"Name": "no transform"}}),
    );
    let rows = |v: &Value| {
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };

    let all = h.ok("query.run", json!({"with": ["Transform"]}));
    assert_eq!(rows(&all), ids);
    let far = h.ok("query.run", json!({"with": ["Transform"], "where": [{"path": "Transform/translation/0", "op": "gt", "value": 2}]}));
    assert_eq!(rows(&far), ids[3..].to_vec());
    let page = h.ok(
        "query.run",
        json!({"with": ["Name"], "without": ["Transform"]}),
    );
    assert_eq!(page["rows"].as_array().unwrap().len(), 1);
    let first = h.ok(
        "query.run",
        json!({"with": ["Transform"], "limit": 2, "select": ["Name"]}),
    );
    assert_eq!(rows(&first), ids[..2].to_vec());
    assert_eq!(first["rows"][0]["components"], json!({"Name": "e0"}));
    let next = h.ok(
        "query.run",
        json!({"with": ["Transform"], "limit": 2, "cursor": first["next_cursor"]}),
    );
    assert_eq!(rows(&next), ids[2..4].to_vec());
    let named = h.ok("query.run", json!({"with": ["Name"], "where": [{"path": "Name", "op": "contains", "value": "transform"}]}));
    assert_eq!(named["rows"].as_array().unwrap().len(), 1);
}

#[test]
fn query_where_rejects_bad_pointer() {
    let mut h = Harness::new().ready();
    let err = h.err("query.run", json!({"with": ["Transform"], "where": [{"path": "Transfrom/translation/0", "op": "eq", "value": 1}]}));
    assert_eq!(err.data.kind, "ComponentNotRegistered");
    let err = h.err(
        "query.run",
        json!({"with": ["Transform"], "where": [{"path": "", "op": "eq", "value": 1}]}),
    );
    assert_eq!(err.code(), -32602);
    let err = h.err("query.run", json!({"with": ["Transform"], "limit": 0}));
    assert_eq!(err.code(), -32602);
}

#[test]
fn component_patch_merges_fields() {
    let mut h = Harness::new().ready();
    let id = h.spawn("A", 1.0);
    let patched = h.ok(
        "component.patch",
        json!({"entity": id, "component": "Transform", "patch": {"scale": [2.0, 2.0, 2.0]}}),
    );
    assert_eq!(patched["value"]["scale"], json!([2.0, 2.0, 2.0]));
    assert_eq!(patched["value"]["translation"], json!([1.0, 0.0, 0.0]));
    h.ok("txn.undo", json!({}));
    let back = h.ok(
        "component.get",
        json!({"entity": id, "component": "Transform"}),
    );
    assert_eq!(back["value"]["scale"], json!([1.0, 1.0, 1.0]));
}

#[test]
fn explicit_transactions_group_ops() {
    let mut h = Harness::new().ready();
    let txn = h.ok("txn.begin", json!({"label": "pair"}))["txn"].clone();
    h.spawn("A", 0.0);
    h.spawn("B", 0.0);
    assert_eq!(h.ok("txn.commit", json!({"txn": txn}))["ops"], 2);
    let undo = h.ok("txn.undo", json!({}));
    assert_eq!(
        (undo["label"].clone(), undo["by"].clone()),
        (json!("pair"), json!("agent:test"))
    );
    assert_eq!(h.ok("scene.info", json!({}))["entity_count"], 0);
    h.ok("txn.redo", json!({}));
    assert_eq!(h.ok("scene.info", json!({}))["entity_count"], 2);
    let txn = h.ok("txn.begin", json!({"label": "nope"}))["txn"].clone();
    h.spawn("C", 0.0);
    h.ok("txn.abort", json!({"txn": txn}));
    assert_eq!(h.ok("scene.info", json!({}))["entity_count"], 2);
}

#[test]
fn scene_save_open_preserves_history_when_unchanged() {
    let mut h = Harness::new().ready();
    let id = h.spawn("A", 1.0);
    h.ok(
        "component.patch",
        json!({"entity": id, "component": "Transform", "patch": {"scale": [3.0, 3.0, 3.0]}}),
    );
    let saved = h.ok("scene.save", json!({}));
    assert_eq!(saved["path"], "scenes/test.scene.ron");
    assert!(h.dir.path().join("scenes/test.scene.ron").is_file());
    assert_eq!(h.ok("scene.info", json!({}))["dirty"], false);

    let dir = std::mem::replace(&mut h.dir, tempfile::tempdir().unwrap());
    drop(h);
    let mut again = Harness::with_config(dir, |_| {});
    again.ok(
        "session.open",
        json!({"client": "test", "actor": "agent:test"}),
    );
    let opened = again.ok("scene.open", json!({"path": "scenes/test.scene.ron"}));
    assert_eq!(
        (opened["history"].clone(), opened["entity_count"].clone()),
        (json!("restored"), json!(1))
    );
    again.ok("txn.undo", json!({}));
    let t = again.ok(
        "component.get",
        json!({"entity": id, "component": "Transform"}),
    );
    assert_eq!(t["value"]["scale"], json!([1.0, 1.0, 1.0]));
}

#[test]
fn scene_new_refuses_dirty_scene_unless_discard() {
    let mut h = Harness::new().ready();
    h.spawn("A", 0.0);
    let err = h.err(
        "scene.new",
        json!({"path": "scenes/b.scene.ron", "name": "B"}),
    );
    assert_eq!(err.data.kind, "SceneDirty");
    h.ok(
        "scene.new",
        json!({"path": "scenes/b.scene.ron", "name": "B", "discard": true}),
    );
    assert_eq!(h.ok("scene.info", json!({}))["name"], "B");
}

#[test]
fn scene_paths_must_stay_inside_the_project() {
    let mut h = Harness::new().ready();
    for path in ["../x.scene.ron", "/tmp/x.scene.ron", "scenes/x.txt"] {
        let err = h.err(
            "scene.new",
            json!({"path": path, "name": "X", "discard": true}),
        );
        assert_eq!(err.code(), -32602, "{path}");
    }
}

#[test]
fn schema_get_returns_types_methods_and_index() {
    let mut h = Harness::new();
    let transform = h.ok("schema.get", json!({"name": "Transform"}))["schema"].clone();
    assert_eq!(transform["title"], "Transform");
    let method = h.ok("schema.get", json!({"name": "entity.spawn"}))["schema"].clone();
    assert_eq!(method["mutates"], true);
    assert_eq!(method["params"]["type"], "object");
    let index = h.ok("schema.get", json!({}))["schema"].clone();
    assert!(
        index["types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "Velocity")
    );
    assert!(
        index["methods"]
            .as_array()
            .unwrap()
            .contains(&json!("query.run"))
    );
}

#[test]
fn engine_exposes_current_scene_text_without_saving() {
    let h = Harness::new();
    assert_eq!(h.engine.scene_text().unwrap_err().data.kind, "NoSceneOpen");
    let mut h = h.ready();
    h.spawn("A", 1.0);
    let text = h.engine.scene_text().unwrap();
    assert!(text.starts_with("// Concordia scene, format 1."), "{text}");
    assert!(text.contains("\"Name\": \"A\""), "{text}");
    assert!(
        !h.dir.path().join("scenes/test.scene.ron").exists(),
        "nothing is written"
    );
    h.ok("scene.save", json!({}));
    let file = std::fs::read_to_string(h.dir.path().join("scenes/test.scene.ron")).unwrap();
    assert_eq!(h.engine.scene_text().unwrap(), file);
}

#[test]
fn scene_text_method_returns_the_open_scene_as_saved() {
    let mut h = Harness::new();
    assert_eq!(h.err("scene.text", json!({})).data.kind, "NoSceneOpen");
    let mut h = h.ready();
    h.spawn("A", 1.0);
    let unsaved = h.ok("scene.text", json!({}));
    assert_eq!(unsaved["path"], "scenes/test.scene.ron");
    assert!(
        unsaved["text"]
            .as_str()
            .unwrap()
            .contains("\"Name\": \"A\""),
        "{unsaved}"
    );
    h.ok("scene.save", json!({}));
    let file = std::fs::read_to_string(h.dir.path().join("scenes/test.scene.ron")).unwrap();
    assert_eq!(h.ok("scene.text", json!({}))["text"], file.as_str());
}
