//! Sessions, default actors and transaction ownership (M0 phase 0.6).

mod common;

use std::time::Duration;

use common::Harness;
use serde_json::json;

fn journal_kinds(h: &Harness) -> Vec<String> {
    let dir = h.dir.path().join(".concordia/journal");
    let file = std::fs::read_dir(dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::read_to_string(file)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .map(|v| {
            format!(
                "{}:{}",
                v["kind"].as_str().unwrap(),
                v["reason"].as_str().unwrap_or("")
            )
        })
        .collect()
}

#[test]
fn sessions_get_unique_ids_and_independent_default_actors() {
    let mut h = Harness::new().ready();
    let other = h.engine.open_session();
    assert_ne!(other, h.session);
    let opened = h
        .engine
        .call(
            other,
            "session.open",
            json!({"client": "b", "actor": "agent:b"}),
        )
        .unwrap();
    assert_eq!(opened["session"], other.to_string());
    assert_eq!(opened["protocol"], 1);
    h.spawn("A", 0.0);
    h.engine
        .call(other, "entity.spawn", json!({"components": {}}))
        .unwrap();
    let entries = h.ok("txn.history", json!({}))["entries"].clone();
    assert_eq!(entries[0]["actor"], "agent:test");
    assert_eq!(entries[1]["actor"], "agent:b");
}

#[test]
fn open_txn_auto_aborts_on_session_close() {
    let mut h = Harness::new().ready();
    let before = h.ok("scene.info", json!({}))["entity_count"].clone();
    let other = h.engine.open_session();
    h.engine
        .call(
            other,
            "session.open",
            json!({"client": "b", "actor": "agent:b"}),
        )
        .unwrap();
    h.engine
        .call(other, "txn.begin", json!({"label": "left open"}))
        .unwrap();
    h.engine
        .call(other, "entity.spawn", json!({"components": {}}))
        .unwrap();
    h.engine.close_session(other);
    assert_eq!(h.ok("scene.info", json!({}))["entity_count"], before);
    h.spawn("after", 0.0);
    assert_eq!(journal_kinds(&h), vec!["abort:session_closed", "commit:"]);
}

#[test]
fn open_txn_times_out() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::with_config(dir, |c| c.txn_timeout = Duration::from_millis(50)).ready();
    h.ok("txn.begin", json!({"label": "slow"}));
    h.spawn("A", 0.0);
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(h.ok("scene.info", json!({}))["entity_count"], 0);
    let err = h.err(
        "txn.commit",
        json!({"txn": "0190a000-0000-7000-8000-000000000000"}),
    );
    assert_eq!(err.data.kind, "TxnNotFound");
    assert_eq!(journal_kinds(&h), vec!["abort:timeout"]);
}
