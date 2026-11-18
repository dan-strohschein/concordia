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
    });
}
