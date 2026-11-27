//! Text-first gameplay graphs (M4 4.9): the `.graph.ron` format, the node library built from what
//! the engine exposes, and a type checker whose diagnostics an agent can act on.

mod check;
mod edit;
mod error;
pub mod fns;
mod format;
pub mod ir;
mod library;
pub mod runtime;
mod types;

pub use check::{Code, Diagnostic, Report, check};
pub use edit::{EditError, GraphEdit, apply_edits, fresh_id, new_graph};
pub use error::GraphError;
pub use format::{
    ExecLink, Graph, GraphKind, Input, Literal, Node, PinDecl, Signature, Variable, read_graph,
    write_graph,
};
pub use ir::{CompiledFunction, compile_event, compile_function};
pub use library::{NodeDef, NodeLibrary, PinDef};
pub use types::{GenericKind, PinType};
