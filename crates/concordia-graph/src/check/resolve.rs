//! Each node's definition: the library's, a variable's, or the graph signature's.

use std::collections::BTreeMap;

use crate::library::{flow, out, pure, req};
use crate::{Graph, Node, NodeDef, NodeLibrary, PinDecl, PinDef, PinType};

use super::{Code, Diagnostic};

/// The definitions of the nodes that resolve; diagnostics for those that don't.
pub(super) fn resolve<'g>(
    graph: &'g Graph,
    lib: &NodeLibrary,
    diags: &mut Vec<Diagnostic>,
) -> BTreeMap<&'g str, NodeDef> {
    let mut defs = BTreeMap::new();
    for (id, node) in &graph.nodes {
        match resolve_node(graph, lib, id, node) {
            Ok(def) => {
                defs.insert(id.as_str(), def);
            }
            Err(d) => diags.push(d),
        }
    }
    defs
}

fn resolve_node(
    graph: &Graph,
    lib: &NodeLibrary,
    id: &str,
    node: &Node,
) -> Result<NodeDef, Diagnostic> {
    let op = node.op.as_str();
    if let Some(name) = op.strip_prefix("var.get.") {
        return variable(graph, id, op, name).map(|ty| {
            pure(
                op,
                "Variables",
                "Reads a graph variable.",
                vec![],
                vec![out("value", ty)],
            )
        });
    }
    if let Some(name) = op.strip_prefix("var.set.") {
        return variable(graph, id, op, name).map(|ty| {
            flow(
                op,
                "Variables",
                "Writes a graph variable.",
                (&["in"], &["out"]),
                vec![req("value", ty)],
                vec![],
            )
        });
    }
    let def = lib.get(op).ok_or_else(|| {
        Diagnostic::at(
            Code::unknown_op,
            Some(id),
            None,
            format!("no node `{op}` in the library"),
        )
        .suggesting(op, lib.ops())
    })?;
    match op {
        "graph.input" => Ok(NodeDef {
            outputs: pins(id, &graph.signature.inputs, false)?,
            ..def.clone()
        }),
        "graph.output" => Ok(NodeDef {
            inputs: pins(id, &graph.signature.outputs, true)?,
            ..def.clone()
        }),
        _ => Ok(def.clone()),
    }
}

/// The declared type of variable `name`.
fn variable(graph: &Graph, id: &str, op: &str, name: &str) -> Result<PinType, Diagnostic> {
    let Some(var) = graph.variables.iter().find(|v| v.name == name) else {
        let prefix = &op[..op.len() - name.len()];
        let ops: Vec<String> = graph
            .variables
            .iter()
            .map(|v| format!("{prefix}{}", v.name))
            .collect();
        return Err(Diagnostic::at(
            Code::unknown_variable,
            Some(id),
            None,
            format!("`{op}` names no declared variable"),
        )
        .suggesting(op, ops.iter().map(String::as_str)));
    };
    parse_type(&var.ty, Some(id), &format!("variable `{name}`"))
}

/// A signature's pins as node pins.
fn pins(id: &str, decls: &[PinDecl], required: bool) -> Result<Vec<PinDef>, Diagnostic> {
    decls
        .iter()
        .map(|d| {
            let ty = parse_type(&d.ty, Some(id), &format!("signature pin `{}`", d.name))?;
            Ok(PinDef {
                name: d.name.clone(),
                ty,
                required,
            })
        })
        .collect()
}

/// Parses a declared type name.
pub(super) fn parse_type(
    text: &str,
    node: Option<&str>,
    what: &str,
) -> Result<PinType, Diagnostic> {
    PinType::parse(text)
        .map_err(|e| Diagnostic::at(Code::unknown_type, node, None, format!("{what}: {e}")))
}
