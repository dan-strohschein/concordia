//! Structural checks: pins exist, links reach real nodes and pins, required inputs are set,
//! exec links join exec pins, and data links don't loop.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Graph, Input, NodeDef};

use super::{Code, Diagnostic};

/// Unknown pins and nodes, unset required inputs.
pub(super) fn data_pins(
    graph: &Graph,
    defs: &BTreeMap<&str, NodeDef>,
    diags: &mut Vec<Diagnostic>,
) {
    for (id, def) in defs {
        let node = &graph.nodes[*id];
        for (pin, input) in &node.inputs {
            if def.input(pin).is_none() {
                diags.push(
                    Diagnostic::at(
                        Code::unknown_pin,
                        Some(id),
                        Some(pin),
                        format!("`{}` has no input `{pin}`", def.op),
                    )
                    .suggesting(pin, def.inputs.iter().map(|p| p.name.as_str())),
                );
                continue;
            }
            if let Input::Link(src, src_pin) = input {
                link_source(graph, defs, (id, pin), (src, src_pin), diags);
            }
        }
        for p in def
            .inputs
            .iter()
            .filter(|p| p.required && !node.inputs.contains_key(&p.name))
        {
            diags.push(Diagnostic::at(
                Code::unconnected_input,
                Some(id),
                Some(&p.name),
                format!(
                    "`{}` needs `{}` ({}): link it or give it a value",
                    def.op, p.name, p.ty
                ),
            ));
        }
    }
}

/// The linked node exists and has the output.
fn link_source(
    graph: &Graph,
    defs: &BTreeMap<&str, NodeDef>,
    (id, pin): (&str, &str),
    (src, src_pin): (&str, &str),
    diags: &mut Vec<Diagnostic>,
) {
    if !graph.nodes.contains_key(src) {
        diags.push(Diagnostic::at(
            Code::unknown_node,
            Some(id),
            Some(pin),
            format!("`{pin}` links to a missing node {src}"),
        ));
        return;
    }
    // An unresolved source has its own diagnostic.
    let Some(src_def) = defs.get(src) else {
        return;
    };
    if src_def.output(src_pin).is_none() {
        diags.push(
            Diagnostic::at(
                Code::unknown_pin,
                Some(id),
                Some(pin),
                format!(
                    "`{pin}` links to `{}`'s output `{src_pin}`, which it doesn't have",
                    src_def.op
                ),
            )
            .suggesting(src_pin, src_def.outputs.iter().map(|p| p.name.as_str())),
        );
    }
}

/// Exec links join an exec output to an exec input.
pub(super) fn exec_links(
    graph: &Graph,
    defs: &BTreeMap<&str, NodeDef>,
    diags: &mut Vec<Diagnostic>,
) {
    for link in &graph.exec {
        for (node, pin, outgoing) in [(&link.0, &link.1, true), (&link.2, &link.3, false)] {
            if !graph.nodes.contains_key(node) {
                diags.push(Diagnostic::at(
                    Code::unknown_node,
                    Some(node),
                    Some(pin),
                    format!("an exec link names a missing node {node}"),
                ));
                continue;
            }
            let Some(def) = defs.get(node.as_str()) else {
                continue;
            };
            let pins = if outgoing {
                &def.exec_out
            } else {
                &def.exec_in
            };
            if pins.iter().any(|p| p == pin) {
                continue;
            }
            let side = if outgoing {
                "exec output"
            } else {
                "exec input"
            };
            let d = match pins.is_empty() {
                true => Diagnostic::at(
                    Code::exec_mismatch,
                    Some(node),
                    Some(pin),
                    format!("`{}` has no {side}s", def.op),
                ),
                false => Diagnostic::at(
                    Code::unknown_pin,
                    Some(node),
                    Some(pin),
                    format!("`{}` has no {side} `{pin}`", def.op),
                )
                .suggesting(pin, pins.iter().map(String::as_str)),
            };
            diags.push(d);
        }
    }
}

/// The nodes in data order (sources first); a diagnostic per looping link. Nodes on or after a
/// loop are left out.
pub(super) fn data_order<'g>(
    graph: &'g Graph,
    defs: &BTreeMap<&'g str, NodeDef>,
    diags: &mut Vec<Diagnostic>,
) -> Vec<&'g str> {
    let mut state = BTreeMap::new();
    let mut order = Vec::new();
    let mut looping = BTreeSet::new();
    for id in defs.keys() {
        visit(graph, defs, id, &mut state, &mut order, &mut looping, diags);
    }
    order.retain(|id| !looping.contains(id));
    order
}

#[derive(Clone, Copy, PartialEq)]
enum Mark {
    Open,
    Done,
}

fn visit<'g>(
    graph: &'g Graph,
    defs: &BTreeMap<&'g str, NodeDef>,
    id: &'g str,
    state: &mut BTreeMap<&'g str, Mark>,
    order: &mut Vec<&'g str>,
    looping: &mut BTreeSet<&'g str>,
    diags: &mut Vec<Diagnostic>,
) {
    if state.contains_key(id) {
        return;
    }
    state.insert(id, Mark::Open);
    for (pin, input) in &graph.nodes[id].inputs {
        let Input::Link(src, _) = input else { continue };
        let Some((src, _)) = defs.get_key_value(src.as_str()) else {
            continue;
        };
        match state.get(src) {
            Some(Mark::Open) => {
                looping.insert(id);
                looping.insert(src);
                diags.push(Diagnostic::at(
                    Code::data_cycle,
                    Some(id),
                    Some(pin),
                    format!("`{pin}` closes a loop of data links"),
                ));
            }
            Some(Mark::Done) => {
                if looping.contains(src) {
                    looping.insert(id);
                }
            }
            None => {
                visit(graph, defs, src, state, order, looping, diags);
                if looping.contains(src) {
                    looping.insert(id);
                }
            }
        }
    }
    state.insert(id, Mark::Done);
    order.push(id);
}
