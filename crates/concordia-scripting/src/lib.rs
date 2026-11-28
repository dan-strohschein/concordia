//! The scripts engine module (M2 phase 2.6d, ADR-0061/0062): a project's gameplay modules,
//! configured by `[scripts]` in `concordia.toml`.
//!
//! - At engine start, each module is loaded (WASM components and native libraries from disk,
//!   or modules linked into the engine) and its types and traits registered, so scenes and the
//!   API can use script components by name. A module that fails to load is logged and skipped.
//! - When play starts, every module's processors join the play schedule.
//! - `script.reload` reloads what is on disk (it never builds): changed component types are
//!   migrated in the edit world as one undoable transaction by `script:<module>`, and in the
//!   play world, all or nothing.

mod config;
mod fingerprint;
mod graph_api;
mod module;
mod reload;
mod state;

pub use config::{ScriptEntry, ScriptKind, ScriptsConfig};
pub use graph_api::{
    CompiledProcessor, GraphApply, GraphApplyParams, GraphApplyResult, GraphCheck,
    GraphCheckParams, GraphCheckResult, GraphCompile, GraphCompileParams, GraphCompileResult,
    GraphListNodes, GraphListNodesParams, GraphListNodesResult, GraphRead, GraphReadParams,
    GraphReadResult, GraphTrace, GraphTraceParams, GraphTraceResult,
};
pub use module::ScriptsModule;
pub use reload::{
    MigratedType, ReloadedModule, ScriptReload, ScriptReloadParams, ScriptReloadResult,
};
