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

mod debug_draw;
mod engine;
mod error;
mod events;
mod handle;
mod image;
mod link;
mod map;
mod method;
mod methods;
mod module;
mod patch;
mod rpc;
mod templates;
pub mod transport;
mod ui_draw;

pub use concordia_terra_sim::Template;
pub use debug_draw::{DebugGeometry, DebugLine, DebugTriangle};
pub use engine::{DocumentEdit, Engine, EngineConfig};
pub use error::{ApiError, ApiErrorKind, ErrorData};
pub use events::{EventSink, Topic};
pub use handle::{EngineHandle, join_engine};
pub use image::{EncodedImage, image_fields};
pub use link::EngineLink;
pub use method::{
    Audience, CallCtx, Method, MethodInfo, MethodRegistry, methods, params_schema, result_schema,
};
pub use methods::{EntityRef, WorldSel};
pub use module::{EngineModule, Fingerprint, PlaySetup};
pub use patch::merge_patch;
pub use ui_draw::{
    UI_BOUNDS_LAYER, UiDrawList, UiFrame, UiMask, UiPaint, UiQuad, UiStop, UiTexture,
    UiTextureFormat,
};

/// Engine API protocol version reported by `session.open`.
pub const PROTOCOL_VERSION: u32 = 1;
