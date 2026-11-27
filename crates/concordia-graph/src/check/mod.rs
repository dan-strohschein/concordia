//! The type checker (M4 4.9, H7–H9): every problem as a located [`Diagnostic`].

mod diagnostic;
pub(crate) mod infer;
mod resolve;
mod rules;
mod structure;

pub use diagnostic::{Code, Diagnostic};

use crate::{Graph, NodeLibrary, PinType};

/// What checking a graph found.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Every problem, in a stable order.
    pub diagnostics: Vec<Diagnostic>,
    types: infer::PinTypes,
}

impl Report {
    /// No problems.
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// The inferred type of `node`'s output `pin`.
    pub fn pin_type(&self, node: &str, pin: &str) -> Option<&PinType> {
        self.types.get(&(node.to_owned(), pin.to_owned()))
    }
}

/// Checks `graph` against `lib`.
pub fn check(graph: &Graph, lib: &NodeLibrary) -> Report {
    let mut diagnostics = Vec::new();
    let defs = resolve::resolve(graph, lib, &mut diagnostics);
    structure::data_pins(graph, &defs, &mut diagnostics);
    structure::exec_links(graph, &defs, &mut diagnostics);
    let order = structure::data_order(graph, &defs, &mut diagnostics);
    let types = infer::infer(graph, &defs, &order, &mut diagnostics);
    rules::purity(graph, &defs, &mut diagnostics);
    rules::kind(graph, &defs, &mut diagnostics);
    Report { diagnostics, types }
}
