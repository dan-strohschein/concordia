//! Compiled gameplay graphs as script modules (M4 4.10b, J1): a `.graph.ron` Event graph loads as
//! a [`ScriptModule`](concordia_script::ScriptModule) whose processors run compiled WASM (or the
//! interpreter) over the batch's wire columns, through the same host as every script.

mod backend;
mod module;

pub use backend::{GraphBackend, Mode, engine_library};
pub use module::{GraphModule, TickTrace};
