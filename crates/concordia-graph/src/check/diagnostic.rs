//! Diagnostics (M4 4.9, H8): a code, where (node and pin) and what to do about it.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// What kind of problem a diagnostic reports.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Reflect)]
pub enum Code {
    /// The node's `op` isn't in the library.
    unknown_op,
    /// A `var.get`/`var.set` names a variable the graph doesn't declare.
    unknown_variable,
    /// A type name (a variable's or the signature's) doesn't parse.
    unknown_type,
    /// A pin the node (or the linked node) doesn't have.
    unknown_pin,
    /// A link to a node the graph doesn't have.
    unknown_node,
    /// A link or value of the wrong type.
    type_mismatch,
    /// A required input with no link and no value.
    unconnected_input,
    /// Data links that loop.
    data_cycle,
    /// A node with side effects in a pure graph.
    impure_in_pure,
    /// A generic pin bound to a type its constraint doesn't admit.
    generic_unresolved,
    /// The graph breaks a rule of its kind.
    kind_rule,
    /// An exec link from or to a node that has no such control flow.
    exec_mismatch,
    /// A node or type the compiler can't lower yet (M4 4.10).
    unsupported_node,
}

/// One problem, located.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Diagnostic {
    /// What kind of problem.
    pub code: Code,
    /// The node to fix (none: the graph as a whole).
    pub node_id: Option<String>,
    /// The pin to fix.
    pub pin: Option<String>,
    /// What's wrong.
    pub message: String,
    /// A likely fix (the closest names).
    pub suggestion: Option<String>,
}

impl Diagnostic {
    /// A diagnostic at `node` (and `pin`).
    pub(crate) fn at(code: Code, node: Option<&str>, pin: Option<&str>, message: String) -> Self {
        Self {
            code,
            node_id: node.map(str::to_owned),
            pin: pin.map(str::to_owned),
            message,
            suggestion: None,
        }
    }

    /// With the closest of `candidates` to `input` as the suggestion.
    #[must_use]
    pub(crate) fn suggesting<'a>(
        mut self,
        input: &str,
        candidates: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let close = concordia_core::suggest::did_you_mean(input, candidates);
        self.suggestion = (!close.is_empty()).then(|| close.join(", "));
        self
    }
}
