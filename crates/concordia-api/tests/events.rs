//! `events.subscribe`: change feeds pushed to sessions with an event sink (M1 phase 1.9).

mod common;

use std::sync::{Arc, Mutex};

use common::Harness;
use concordia_core::id::SessionId;
use serde_json::{Value, json};
use tracing_subscriber::layer::SubscriberExt;

/// Gives `session` an event sink collecting every pushed notification.
fn sink(h: &mut Harness, session: SessionId) -> Arc<Mutex<Vec<Value>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let out = seen.clone();
    h.engine.set_event_sink(
        session,
        Box::new(move |event| out.lock().unwrap().push(event)),
    );
    seen
}

fn events(seen: &Arc<Mutex<Vec<Value>>>) -> Vec<Value> {
    std::mem::take(&mut *seen.lock().unwrap())
}

#[test]
fn subscribers_receive_edit_events_with_actor_and_entities() {
    let mut h = Harness::new().ready();
    let session = h.session;
    let seen = sink(&mut h, session);
    h.ok("events.subscribe", json!({"topics": ["edit"]}));
    let id = h.spawn("Crate", 1.0);
    let got = events(&seen);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0]["jsonrpc"], "2.0");
    assert_eq!(got[0]["method"], "events.event");
    let p = &got[0]["params"];
    assert_eq!(
        (p["topic"].as_str(), p["method"].as_str()),
        (Some("edit"), Some("entity.spawn"))
    );
    assert_eq!(p["actor"], "agent:test");
    assert_eq!(p["entities"], json!([id]));
}

#[test]
fn edits_by_one_session_reach_another_sessions_subscription() {
    let mut h = Harness::new().ready();
    let editor = h.engine.open_session();
    let seen = sink(&mut h, editor);
    h.engine
        .call(editor, "events.subscribe", json!({"topics": ["edit"]}))
        .unwrap();
    let id = h.spawn("Crate", 1.0);
    h.ok(
        "component.patch",
        json!({"entity": id, "component": "Transform", "patch": {"translation": [5.0, 0.0, 0.0]}}),
    );
    let got = events(&seen);
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[1]["params"]["method"], "component.patch");
    assert_eq!(got[1]["params"]["entities"], json!([id]));
}

#[test]
fn transactions_emit_one_event_on_commit_and_undo_emits_one() {
    let mut h = Harness::new().ready();
    let session = h.session;
    let seen = sink(&mut h, session);
    h.ok("events.subscribe", json!({"topics": ["edit"]}));
    let txn = h.ok("txn.begin", json!({"label": "two crates"}))["txn"].clone();
    let a = h.spawn("A", 1.0);
    let b = h.spawn("B", 2.0);
    assert!(events(&seen).is_empty(), "nothing is visible before commit");
    h.ok("txn.commit", json!({"txn": txn}));
    let got = events(&seen);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0]["params"]["label"], "two crates");
    let mut entities: Vec<Value> = got[0]["params"]["entities"].as_array().unwrap().clone();
    entities.sort_by_key(|v| v.to_string());
    let mut expected = vec![json!(a), json!(b)];
    expected.sort_by_key(|v| v.to_string());
    assert_eq!(entities, expected);
    h.ok("txn.undo", json!({}));
    let undo = events(&seen);
    assert_eq!(undo.len(), 1, "{undo:?}");
    assert_eq!(
        (
            undo[0]["params"]["method"].as_str(),
            undo[0]["params"]["undo"].as_bool()
        ),
        (Some("txn.undo"), Some(true))
    );
}

#[test]
fn only_subscribed_topics_are_delivered_and_unsubscribe_stops_them() {
    let mut h = Harness::new().ready();
    let session = h.session;
    let seen = sink(&mut h, session);
    h.ok("events.subscribe", json!({"topics": ["sim"]}));
    h.spawn("Crate", 1.0);
    assert!(events(&seen).is_empty(), "edit events are not subscribed");
    h.ok("sim.step", json!({"ticks": 3}));
    let got = events(&seen);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(
        (
            got[0]["params"]["topic"].as_str(),
            got[0]["params"]["tick"].as_u64()
        ),
        (Some("sim"), Some(3))
    );
    h.ok("events.unsubscribe", json!({}));
    h.ok("sim.step", json!({"ticks": 1}));
    assert!(events(&seen).is_empty());
}

#[test]
fn scene_events_report_open_and_save() {
    let mut h = Harness::new().ready();
    let session = h.session;
    let seen = sink(&mut h, session);
    h.ok("events.subscribe", json!({"topics": ["scene"]}));
    h.spawn("Crate", 1.0);
    h.ok("scene.save", json!({}));
    let got = events(&seen);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(
        (
            got[0]["params"]["topic"].as_str(),
            got[0]["params"]["event"].as_str()
        ),
        (Some("scene"), Some("saved"))
    );
}

#[test]
fn unknown_topics_and_sinkless_sessions_are_errors() {
    let mut h = Harness::new().ready();
    let no_sink = h.err("events.subscribe", json!({"topics": ["edit"]}));
    assert!(no_sink.message.contains("cannot push"), "{no_sink:?}");
    assert!(
        no_sink
            .data
            .suggestions
            .iter()
            .any(|s| s.contains("log.tail")),
        "{no_sink:?}"
    );
    let session = h.session;
    let _ = sink(&mut h, session);
    let bad = h.err("events.subscribe", json!({"topics": ["edits"]}));
    assert_eq!(bad.data.kind, "InvalidParams", "{bad:?}");
}

#[test]
fn closing_a_session_drops_its_sink() {
    let mut h = Harness::new().ready();
    let editor = h.engine.open_session();
    let seen = sink(&mut h, editor);
    h.engine
        .call(editor, "events.subscribe", json!({"topics": ["edit"]}))
        .unwrap();
    h.engine.close_session(editor);
    h.spawn("Crate", 1.0);
    assert!(events(&seen).is_empty());
}

#[test]
fn pending_topic_reports_each_edit_inside_an_open_transaction() {
    let mut h = Harness::new().ready();
    let session = h.session;
    let seen = sink(&mut h, session);
    h.ok("events.subscribe", json!({"topics": ["pending"]}));
    let txn = h.ok("txn.begin", json!({"label": "drag"}))["txn"].clone();
    let a = h.spawn("A", 1.0);
    h.ok(
        "component.patch",
        json!({"entity": a, "component": "Transform", "patch": {"scale": [2.0, 2.0, 2.0]}}),
    );
    let got = events(&seen);
    assert_eq!(got.len(), 2, "{got:?}");
    for (event, method) in got.iter().zip(["entity.spawn", "component.patch"]) {
        let p = &event["params"];
        assert_eq!(
            (p["topic"].as_str(), p["method"].as_str()),
            (Some("pending"), Some(method))
        );
        assert_eq!(
            (&p["txn"], &p["label"], &p["entities"]),
            (&txn, &json!("drag"), &json!([a]))
        );
        assert_eq!(p["actor"], "agent:test");
    }
    h.ok("txn.commit", json!({"txn": txn}));
    assert!(
        events(&seen).is_empty(),
        "commits go to the edit topic, not pending"
    );
}

#[test]
fn pending_topic_reports_an_abort_with_the_reverted_entities() {
    let mut h = Harness::new().ready();
    let session = h.session;
    let seen = sink(&mut h, session);
    let txn = h.ok("txn.begin", json!({"label": "drag"}))["txn"].clone();
    let a = h.spawn("A", 1.0);
    h.ok("events.subscribe", json!({"topics": ["pending"]}));
    h.ok("txn.abort", json!({"txn": txn}));
    let got = events(&seen);
    assert_eq!(got.len(), 1, "{got:?}");
    let p = &got[0]["params"];
    assert_eq!(
        (p["method"].as_str(), p["aborted"].as_bool()),
        (Some("txn.abort"), Some(true))
    );
    assert_eq!(p["entities"], json!([a]));
}

#[test]
fn edits_outside_transactions_are_not_pending() {
    let mut h = Harness::new().ready();
    let session = h.session;
    let seen = sink(&mut h, session);
    h.ok("events.subscribe", json!({"topics": ["pending"]}));
    h.spawn("A", 1.0);
    assert!(
        events(&seen).is_empty(),
        "an auto-committed edit is never pending"
    );
}

#[test]
fn log_topic_announces_new_log_records() {
    let ring = std::sync::Arc::new(concordia_core::log::LogRing::new(
        std::num::NonZeroUsize::new(64).unwrap(),
    ));
    let dir = tempfile::tempdir().unwrap();
    let r = ring.clone();
    let mut h = Harness::with_config(dir, move |c| c.log_ring = Some(r)).ready();
    let session = h.session;
    let seen = sink(&mut h, session);
    h.ok("events.subscribe", json!({"topics": ["log"]}));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || h.spawn("Logged", 0.0));
    let got = events(&seen);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0]["params"]["topic"], "log");
    assert_eq!(
        got[0]["params"]["latest"].as_u64(),
        Some(ring.next_seq() - 1),
        "the newest record's seq"
    );
    h.ok("scene.info", json!({}));
    assert!(events(&seen).is_empty(), "no new records, no event");
}
