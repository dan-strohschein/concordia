//! Structured edits to a graph (M4 4.10c, J18): what `graph.apply` applies, all or nothing.
//! Edits keep the graph well formed (ids, links to existing nodes); types are the checker's.

use std::collections::BTreeMap;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::format::v7;
use crate::{ExecLink, Graph, GraphKind, Input, Literal, Node, Signature, Variable};

/// One edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum GraphEdit {
    /// Adds a node (a fresh UUIDv7 id when `id` is absent), optionally placed in the editor.
    AddNode {
        /// Its id (UUIDv7).
        id: Option<String>,
        /// Its operation.
        op: String,
        /// Its position.
        at: Option<(f32, f32)>,
    },
    /// Moves a node in the editor (layout only: never changes what the graph means).
    SetPosition {
        /// The node.
        node: String,
        /// Its position.
        at: (f32, f32),
    },
    /// Removes a node, its layout and every link to or from it.
    RemoveNode {
        /// The node.
        id: String,
    },
    /// Links `from`'s output to `to`'s input (replacing what was there).
    Connect {
        /// Source node.
        from: String,
        /// Its output pin.
        from_pin: String,
        /// Target node.
        to: String,
        /// Its input pin.
        to_pin: String,
    },
    /// Clears an input (link or value).
    Disconnect {
        /// The node.
        to: String,
        /// The input pin.
        to_pin: String,
    },
    /// Adds control flow from `from`'s exec output to `to`'s exec input.
    ConnectExec {
        /// Source node.
        from: String,
        /// Its exec output.
        from_pin: String,
        /// Target node.
        to: String,
        /// Its exec input.
        to_pin: String,
    },
    /// Removes that control flow.
    DisconnectExec {
        /// Source node.
        from: String,
        /// Its exec output.
        from_pin: String,
        /// Target node.
        to: String,
        /// Its exec input.
        to_pin: String,
    },
    /// Gives an input a constant.
    SetValue {
        /// The node.
        node: String,
        /// The input pin.
        pin: String,
        /// The constant.
        value: Literal,
    },
    /// Declares a variable (or replaces its declaration).
    SetVariable {
        /// Name.
        name: String,
        /// Type.
        ty: String,
        /// Starting value.
        default: Option<Literal>,
    },
    /// Removes a variable.
    RemoveVariable {
        /// Name.
        name: String,
    },
}

/// Why edit `index` couldn't apply (nothing was changed).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("edit {index}: {message}")]
pub struct EditError {
    /// Which edit (zero-based).
    pub index: usize,
    /// What's wrong.
    pub message: String,
}

/// An empty graph of `kind` with id `id` (a UUIDv7).
pub fn new_graph(kind: GraphKind, id: String) -> Graph {
    Graph {
        id,
        kind,
        version: 1,
        signature: Signature::default(),
        variables: Vec::new(),
        nodes: BTreeMap::new(),
        exec: Vec::new(),
        layout: BTreeMap::new(),
    }
}

/// A fresh UUIDv7 as text.
pub fn fresh_id() -> String {
    uuid::Uuid::now_v7().hyphenated().to_string()
}

/// Applies `edits` in order; on any failure `graph` is unchanged. Returns the added nodes' ids.
pub fn apply_edits(graph: &mut Graph, edits: &[GraphEdit]) -> Result<Vec<String>, EditError> {
    let mut g = graph.clone();
    let mut added = Vec::new();
    for (index, edit) in edits.iter().enumerate() {
        apply(&mut g, edit, &mut added).map_err(|message| EditError { index, message })?;
    }
    g.exec.sort();
    *graph = g;
    Ok(added)
}

fn node<'g>(g: &'g mut Graph, id: &str) -> Result<&'g mut Node, String> {
    g.nodes.get_mut(id).ok_or_else(|| format!("no node {id}"))
}

fn exists(g: &Graph, id: &str) -> Result<(), String> {
    match g.nodes.contains_key(id) {
        true => Ok(()),
        false => Err(format!("no node {id}")),
    }
}

fn apply(g: &mut Graph, edit: &GraphEdit, added: &mut Vec<String>) -> Result<(), String> {
    match edit {
        GraphEdit::AddNode { id, op, at } => {
            let id = id.clone().unwrap_or_else(fresh_id);
            v7("node", &id).map_err(|e| e.to_string())?;
            if g.nodes.contains_key(&id) {
                return Err(format!("node {id} already exists"));
            }
            if op.trim().is_empty() {
                return Err("a node needs an op".into());
            }
            g.nodes.insert(id.clone(), Node::new(op));
            if let Some(at) = at {
                g.layout.insert(id.clone(), *at);
            }
            added.push(id);
        }
        GraphEdit::SetPosition { node: n, at } => {
            exists(g, n)?;
            g.layout.insert(n.clone(), *at);
        }
        GraphEdit::RemoveNode { id } => {
            exists(g, id)?;
            g.nodes.remove(id);
            g.layout.remove(id);
            g.exec.retain(|e| e.0 != *id && e.2 != *id);
            for n in g.nodes.values_mut() {
                n.inputs
                    .retain(|_, i| !matches!(i, Input::Link(src, _) if src == id));
            }
        }
        GraphEdit::Connect {
            from,
            from_pin,
            to,
            to_pin,
        } => {
            exists(g, from)?;
            node(g, to)?
                .inputs
                .insert(to_pin.clone(), Input::Link(from.clone(), from_pin.clone()));
        }
        GraphEdit::Disconnect { to, to_pin } => {
            node(g, to)?.inputs.remove(to_pin);
        }
        GraphEdit::ConnectExec {
            from,
            from_pin,
            to,
            to_pin,
        } => {
            exists(g, from)?;
            exists(g, to)?;
            let link = ExecLink(from.clone(), from_pin.clone(), to.clone(), to_pin.clone());
            if !g.exec.contains(&link) {
                g.exec.push(link);
            }
        }
        GraphEdit::DisconnectExec {
            from,
            from_pin,
            to,
            to_pin,
        } => {
            let link = ExecLink(from.clone(), from_pin.clone(), to.clone(), to_pin.clone());
            let before = g.exec.len();
            g.exec.retain(|e| *e != link);
            if g.exec.len() == before {
                return Err(format!("no exec link {from}.{from_pin} → {to}.{to_pin}"));
            }
        }
        GraphEdit::SetValue {
            node: n,
            pin,
            value,
        } => {
            node(g, n)?
                .inputs
                .insert(pin.clone(), Input::Value(value.clone()));
        }
        GraphEdit::SetVariable { name, ty, default } => {
            if name.trim().is_empty() {
                return Err("a variable needs a name".into());
            }
            let var = Variable {
                name: name.clone(),
                ty: ty.clone(),
                default: default.clone(),
            };
            match g.variables.iter_mut().find(|v| v.name == *name) {
                Some(v) => *v = var,
                None => g.variables.push(var),
            }
        }
        GraphEdit::RemoveVariable { name } => {
            let before = g.variables.len();
            g.variables.retain(|v| v.name != *name);
            if g.variables.len() == before {
                return Err(format!("no variable {name:?}"));
            }
        }
    }
    Ok(())
}
