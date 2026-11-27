//! `.graph.ron`: a graph's signature, variables, nodes (by UUIDv7 id), exec links and, apart from
//! them, the editor's layout. Canonical through `concordia-scene` (sorted maps and fields); the
//! printer also sorts exec links, so semantic diffs never mix with cosmetic ones.

use std::collections::BTreeMap;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::GraphError;

/// A graph document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    /// The graph's id (UUIDv7).
    pub id: String,
    /// What the graph is.
    pub kind: GraphKind,
    /// Format version.
    pub version: u32,
    /// Inputs and outputs (functions) and purity.
    #[serde(default)]
    pub signature: Signature,
    /// Variables (kept in a component at run time).
    #[serde(default)]
    pub variables: Vec<Variable>,
    /// Nodes by id (UUIDv7; text order is creation order).
    #[serde(default)]
    pub nodes: BTreeMap<String, Node>,
    /// Control flow: (from node, its exec output, to node, its exec input).
    #[serde(default)]
    pub exec: Vec<ExecLink>,
    /// Node positions for the editor (cosmetic: not in the semantic hash).
    #[serde(default)]
    pub layout: BTreeMap<String, (f32, f32)>,
}

/// What a graph is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum GraphKind {
    /// Entry points (`event.on_tick`, `on.<Signal>`): a processor.
    Event,
    /// Callable from other graphs; pure or impure.
    Function,
    /// A behavior tree.
    Behavior,
    /// Utility considerations and response curves.
    Utility,
}

/// A function graph's interface.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct Signature {
    /// Inputs.
    pub inputs: Vec<PinDecl>,
    /// Outputs.
    pub outputs: Vec<PinDecl>,
    /// No side effects and no control flow.
    pub pure: bool,
}

/// A named, typed pin of a signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PinDecl {
    /// Name.
    pub name: String,
    /// Type (e.g. `f32`, `Vec3`, `List<EntityId>`).
    pub ty: String,
}

/// A graph variable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    /// Name.
    pub name: String,
    /// Type.
    pub ty: String,
    /// Starting value (else the type's default).
    #[serde(default)]
    pub default: Option<Literal>,
}

/// A node: its operation and its data inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Node {
    /// The operation (a node library `op`).
    pub op: String,
    /// Data inputs by pin: a link from another node's output, or a value.
    #[serde(default)]
    pub inputs: BTreeMap<String, Input>,
}

/// A data input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Input {
    /// Another node's output: (node id, output pin).
    Link(String, String),
    /// A constant.
    Value(Literal),
}

/// A constant value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Literal {
    /// `bool`.
    Bool(bool),
    /// An integer (`i32` or `i64`).
    Int(i64),
    /// A number (`f32`).
    Float(f64),
    /// `String` (also asset references and enum variants by name).
    Text(String),
    /// `Vec2`.
    Vec2([f32; 2]),
    /// `Vec3`.
    Vec3([f32; 3]),
    /// `Vec4`.
    Vec4([f32; 4]),
}

/// An exec link: (from node, exec output, to node, exec input).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Reflect)]
pub struct ExecLink(pub String, pub String, pub String, pub String);

impl Node {
    /// A node of `op` with no inputs set.
    pub fn new(op: &str) -> Self {
        Self {
            op: op.to_owned(),
            inputs: BTreeMap::new(),
        }
    }

    /// With input `pin` set.
    #[must_use]
    pub fn with_input(mut self, pin: &str, input: Input) -> Self {
        self.inputs.insert(pin.to_owned(), input);
        self
    }
}

impl Graph {
    /// The graph as it prints: exec links sorted.
    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut g = self.clone();
        g.exec.sort();
        g
    }

    /// The hash of what the graph means (hex BLAKE3): canonical, without the layout.
    pub fn semantic_hash(&self) -> Result<String, GraphError> {
        let mut g = self.normalized();
        g.layout.clear();
        let mut hasher = blake3::Hasher::new();
        concordia_reflect::canonical_hash(&g, &mut hasher)
            .map_err(|e| GraphError::Parse(e.to_string()))?;
        Ok(hasher.finalize().to_hex().to_string())
    }

    /// Every id is a UUIDv7.
    pub fn validate_ids(&self) -> Result<(), GraphError> {
        v7("graph", &self.id)?;
        for id in self.nodes.keys() {
            v7("node", id)?;
        }
        for id in self.layout.keys() {
            v7("layout entry", id)?;
        }
        Ok(())
    }
}

pub(crate) fn v7(what: &'static str, id: &str) -> Result<(), GraphError> {
    let bad = || GraphError::InvalidId {
        what,
        id: id.to_owned(),
    };
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| bad())?;
    match parsed.get_version() == Some(uuid::Version::SortRand)
        && parsed.hyphenated().to_string() == id
    {
        true => Ok(()),
        false => Err(bad()),
    }
}

/// Reads `.graph.ron` text (validated against the format and its ids).
pub fn read_graph(text: &str) -> Result<Graph, GraphError> {
    let g = concordia_scene::read_document::<Graph>(text)
        .map_err(|e| GraphError::Parse(e.to_string()))?;
    g.validate_ids()?;
    Ok(g)
}

/// Writes a graph as canonical `.graph.ron` text.
pub fn write_graph(graph: &Graph) -> Result<String, GraphError> {
    graph.validate_ids()?;
    concordia_scene::write_document(&graph.normalized())
        .map_err(|e| GraphError::Parse(e.to_string()))
}
