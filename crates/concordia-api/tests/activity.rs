//! Agent activity (M1 phase 1.15): history names who touched what, and `txn.undo_to` rewinds.

mod common;

use common::Harness;
use concordia_api::ApiErrorKind;
use serde_json::{Value, json};

fn commits(h: &mut Harness) -> Vec<Value> {
    let history = h.ok("txn.history", json!({}));
    history["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "commit")
        .cloned()
        .collect()
}

fn x_of(h: &mut Harness, id: &str) -> f64 {
    h.ok(
        "component.get",
        json!({"entity": id, "component": "Transform"}),
    )["value"]["translation"][0]
        .as_f64()
        .unwrap()
}

#[test]
fn history_entries_name_actor_and_entities() {
    let mut h = Harness::new().ready();
    let a = h.spawn("A", 0.0);
    let b = h.spawn("B", 1.0);
    let txn = h.ok(
        "txn.begin",
        json!({"label": "move both", "actor": "agent:builder"}),
    )["txn"]
        .clone();
    for id in [&a, &b] {
        h.ok(
            "component.patch",
            json!({"entity": id, "component": "Transform", "patch": {"translation": [5.0, 0.0, 0.0]}}),
        );
    }
    h.ok("txn.commit", json!({"txn": txn}));

    let rows = commits(&mut h);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["actor"], "agent:test");
    assert_eq!(rows[0]["entities"], json!([a]));
    let moved = &rows[2];
    assert_eq!(moved["actor"], "agent:builder");
    assert_eq!(moved["label"], "move both");
    let mut both = vec![a.clone(), b.clone()];
    both.sort();
    assert_eq!(moved["entities"], json!(both));
    assert!(
        moved["time_ms"].as_u64().unwrap() > 1_700_000_000_000,
        "{moved}"
    );
    assert_eq!(moved["undone"], false);

    h.ok("txn.undo", json!({"actor": "agent:reviewer"}));
    let history = h.ok("txn.history", json!({}));
    let undo = history["entries"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(undo["kind"], "undo");
    assert_eq!(undo["actor"], "agent:reviewer");
    assert_eq!(undo["entities"], json!(both));
    assert_eq!(commits(&mut h)[2]["undone"], true);
}

#[test]
fn undo_to_reverts_every_later_transaction_attributed_to_the_caller() {
    let mut h = Harness::new().ready();
    let id = h.spawn("Crate", 0.0);
    for x in [1.0, 2.0, 3.0] {
        h.ok(
            "component.patch",
            json!({"entity": id, "component": "Transform", "patch": {"translation": [x, 0.0, 0.0]}}),
        );
    }
    let rows = commits(&mut h);
    let first_move = rows[1]["txn"].clone();

    let result = h.ok(
        "txn.undo_to",
        json!({"txn": first_move, "actor": "human:local"}),
    );
    let undone: Vec<Value> = result["undone"].as_array().unwrap().clone();
    assert_eq!(undone.len(), 3, "{result}");
    assert_eq!(undone[0], rows[3]["txn"], "latest first");
    assert_eq!(undone[2], first_move);
    assert_eq!(result["by"], "human:local");
    assert_eq!(x_of(&mut h, &id), 0.0);

    let history = h.ok("txn.history", json!({}));
    let undos: Vec<&Value> = history["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "undo")
        .collect();
    assert_eq!(undos.len(), 3);
    assert!(undos.iter().all(|u| u["actor"] == "human:local"));

    // Already undone: nothing to rewind to.
    let again = h.err("txn.undo_to", json!({"txn": first_move}));
    assert_eq!(again.kind, ApiErrorKind::TxnNotFound);
    assert!(
        again
            .data
            .suggestions
            .iter()
            .any(|s| s.contains("txn.history"))
    );

    // Redo still replays them in order.
    h.ok("txn.redo", json!({}));
    assert_eq!(x_of(&mut h, &id), 1.0);
}
