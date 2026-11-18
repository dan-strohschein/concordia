//! `log.tail` over the in-memory log ring (M0 phase 0.6).

mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_core::log::LogRing;
use serde_json::json;
use tracing_subscriber::layer::SubscriberExt;

#[test]
fn log_tail_returns_ring_records_and_scene_saved() {
    let ring = Arc::new(LogRing::new(NonZeroUsize::new(64).unwrap()));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let dir = tempfile::tempdir().unwrap();
        let shared = Arc::clone(&ring);
        let mut h = common::Harness::with_config(dir, move |c| c.log_ring = Some(shared)).ready();
        tracing::debug!("noise");
        h.ok("scene.save", json!({}));
        let all = h.ok("log.tail", json!({}));
        let messages: Vec<_> = all["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["message"].as_str().unwrap().to_owned())
            .collect();
        assert!(messages.contains(&"scene saved".to_owned()), "{messages:?}");
        let infos = h.ok("log.tail", json!({"level": "info"}));
        assert!(
            infos["records"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["level"] != "debug")
        );
        let next = all["next"].as_u64().unwrap();
        assert!(
            h.ok("log.tail", json!({"since": next}))["records"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let limited = h.ok("log.tail", json!({"limit": 1}));
        assert_eq!(limited["records"].as_array().unwrap().len(), 1);
        h.ok("scene.open", json!({"path": "scenes/test.scene.ron"}));
        let opened = h.ok("log.tail", json!({"since": limited["next"]}));
        assert!(
            opened["records"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["message"] == "scene opened"),
            "{opened}"
        );
    });
}

#[test]
fn committed_edits_undo_and_redo_are_logged_with_their_actor() {
    let ring = Arc::new(LogRing::new(NonZeroUsize::new(64).unwrap()));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let dir = tempfile::tempdir().unwrap();
        let shared = Arc::clone(&ring);
        let mut h = common::Harness::with_config(dir, move |c| c.log_ring = Some(shared)).ready();
        h.ok(
            "entity.spawn",
            json!({"components": {"Name": "Crate"}, "actor": "human:local"}),
        );
        h.ok("txn.undo", json!({"actor": "agent:test"}));
        let records = h.ok("log.tail", json!({"level": "info"}))["records"].clone();
        let edits: Vec<String> = records
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["target"] == "concordia::edit")
            .map(|r| {
                format!(
                    "{} {}",
                    r["message"].as_str().unwrap(),
                    r["fields"]["actor"].as_str().unwrap_or("?")
                )
            })
            .collect();
        assert_eq!(
            edits,
            ["edit committed human:local", "edit undone human:local"],
            "{records}"
        );
    });
}
