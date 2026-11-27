//! A function described for the graph node library (`#[graph_node]`, M4 4.9).

use crate::TypeRef;

/// A Rust function a graph can call: its name, category, purity, typed inputs and output.
#[derive(Debug, Clone)]
pub struct GraphFnInfo {
    /// The function's name.
    pub name: &'static str,
    /// The library category (`Vector`, `Math`, …).
    pub category: &'static str,
    /// Its doc comment.
    pub docs: &'static str,
    /// No side effects (callable from pure graphs, no exec pins).
    pub pure: bool,
    /// Arguments: (name, type).
    pub inputs: Vec<(&'static str, TypeRef)>,
    /// What it returns (`None`: nothing).
    pub output: Option<TypeRef>,
}
