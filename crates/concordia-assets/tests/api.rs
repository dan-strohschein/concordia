//! `asset.import` and `asset.list` through the Engine API (M1 phase 1.4).

mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_api::{Engine, EngineConfig};
use concordia_assets::AssetsModule;
use concordia_core::log::LogRing;
use serde_json::json;
use tracing_subscriber::layer::SubscriberExt;

#[test]
fn asset_import_via_api_is_attributed_and_idempotent() {
    let ring = Arc::new(LogRing::new(NonZeroUsize::new(64).unwrap()));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let dir = tempfile::tempdir().unwrap();
        common::write_png(&dir.path().join("tex/crate.png"), 8, 0);
        let mut config = EngineConfig::new(dir.path());
        config.threads = NonZeroUsize::MIN;
        config.log_ring = Some(Arc::clone(&ring));
        config.modules = vec![Arc::new(AssetsModule::new(dir.path()))];
        let mut engine = Engine::new(config).unwrap();
        let s = engine.open_session();
        let first = engine
            .call(
                s,
                "asset.import",
                json!({"path": "tex/crate.png", "actor": "agent:tech-artist#1"}),
            )
            .unwrap();
        let again = engine
            .call(
                s,
                "asset.import",
                json!({"path": "tex/crate.png", "actor": "agent:tech-artist#1"}),
            )
            .unwrap();
        assert_eq!(first["id"], again["id"]);
        assert_eq!(again["cached"], true);
        let records = engine
            .call(s, "log.tail", json!({"level": "info"}))
            .unwrap()["records"]
            .clone();
        let imported = records
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["message"] == "asset imported")
            .expect("log record");
        assert_eq!(imported["fields"]["actor"], "agent:tech-artist#1");
        let list = engine.call(s, "asset.list", json!({})).unwrap();
        assert_eq!(list["assets"][0]["path"], "tex/crate.png");
        assert_eq!(list["assets"][0]["importer"], "image");
        let err = engine
            .call(s, "asset.import", json!({"path": "../x.png"}))
            .unwrap_err();
        assert_eq!(err.code(), -32602, "{err:?}");
    });
}
