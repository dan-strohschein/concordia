//! Test harness: a synchronous engine on a temporary project.
#![allow(dead_code, missing_docs)]

use std::num::NonZeroUsize;
use std::time::Duration;

use concordia_api::{ApiError, Engine, EngineConfig};
use concordia_core::id::SessionId;
use serde_json::{Value, json};

pub struct Harness {
    pub dir: tempfile::TempDir,
    pub engine: Engine,
    pub session: SessionId,
}

pub fn config(dir: &tempfile::TempDir) -> EngineConfig {
    let mut config = EngineConfig::new(dir.path());
    config.deterministic_ids = Some(7);
    config.threads = NonZeroUsize::new(1).unwrap();
    config.journal.sync = false;
    config.txn_timeout = Duration::from_secs(120);
    config
}

impl Harness {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        Self::with_config(dir, |_| {})
    }

    pub fn with_config(dir: tempfile::TempDir, tweak: impl FnOnce(&mut EngineConfig)) -> Self {
        let mut cfg = config(&dir);
        tweak(&mut cfg);
        let mut engine = Engine::new(cfg).unwrap();
        let session = engine.open_session();
        Self {
            dir,
            engine,
            session,
        }
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, ApiError> {
        self.engine.call(self.session, method, params)
    }

    pub fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    /// Opens the session as `agent:test` and creates `scenes/test.scene.ron`.
    pub fn ready(mut self) -> Self {
        self.ok(
            "session.open",
            json!({"client": "test", "actor": "agent:test"}),
        );
        self.ok(
            "scene.new",
            json!({"path": "scenes/test.scene.ron", "name": "Test"}),
        );
        self
    }

    pub fn spawn(&mut self, name: &str, x: f32) -> String {
        let components = json!({
            "Name": name,
            "Transform": {"translation": [x, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        });
        self.ok("entity.spawn", json!({ "components": components }))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// Error `data`, panicking if the call succeeded.
    pub fn err(&mut self, method: &str, params: Value) -> ApiError {
        self.call(method, params).expect_err("call should fail")
    }
}
