//! The Concordia Engine API: one typed API for the editor, CLI, tests and agents.
//!
//! JSON-RPC 2.0 methods named `domain.verb`, served over newline-delimited stdio
//! and localhost WebSocket. Every mutating call carries an actor and goes through
//! `concordia-command`; simulation runs only in a separate play world (ADR-0032);
//! errors carry codes, suggestions and schema hints (ADR-0031).
//!
//! ```
//! use concordia_api::{Engine, EngineConfig};
//! use serde_json::json;
//!
//! let dir = std::env::temp_dir().join(format!("concordia-doc-{}", std::process::id()));
//! let mut engine = Engine::new(EngineConfig::new(&dir))?;
//! let session = engine.open_session();
//! engine.call(session, "session.open", json!({"client": "doc", "actor": "agent:doc"}))?;
//! engine.call(session, "scene.new", json!({"path": "scenes/a.scene.ron", "name": "A"}))?;
//! let spawned = engine.call(session, "entity.spawn", json!({"components": {"Name": "Hero"}}))?;
//! assert!(spawned["id"].is_string());
//! # std::fs::remove_dir_all(&dir).ok();
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod engine;
mod error;
mod handle;
mod map;
mod method;
mod methods;
mod module;
mod patch;
mod rpc;
pub mod transport;

pub use engine::{Engine, EngineConfig};
pub use error::{ApiError, ApiErrorKind, ErrorData};
pub use handle::EngineHandle;
pub use method::{
    CallCtx, Method, MethodInfo, MethodRegistry, methods, params_schema, result_schema,
};
pub use methods::{EntityRef, WorldSel};
pub use module::{EngineModule, PlaySetup};
pub use patch::merge_patch;

/// Engine API protocol version reported by `session.open`.
pub const PROTOCOL_VERSION: u32 = 1;
