//! Text-first gameplay graphs (M4 4.9): the `.graph.ron` format, the node library built from what
//! the engine exposes, and a type checker whose diagnostics an agent can act on.

mod check;
mod error;
pub mod fns;
mod format;
mod library;
mod types;

pub use check::{Code, Diagnostic, Report, check};
pub use error::GraphError;
pub use format::{
    ExecLink, Graph, GraphKind, Input, Literal, Node, PinDecl, Signature, Variable, read_graph,
    write_graph,
};
pub use library::{NodeDef, NodeLibrary, PinDef};
pub use types::{GenericKind, PinType};
