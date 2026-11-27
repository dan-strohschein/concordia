//! Purity and the rules of each graph kind (H9).

use std::collections::BTreeMap;

use crate::{Graph, GraphKind, Input, NodeDef, PinType};

use super::resolve::parse_type;
use super::{Code, Diagnostic};

/// Ops that keep a utility score in [0, 1].
const SCORE_OPS: [&str; 2] = ["utility.curve", "math.saturate"];

/// Side-effect-free graphs hold no impure nodes.
pub(super) fn purity(graph: &Graph, defs: &BTreeMap<&str, NodeDef>, diags: &mut Vec<Diagnostic>) {
    if !(graph.signature.pure || graph.kind == GraphKind::Utility) {
        return;
    }
    for (id, def) in defs.iter().filter(|(_, d)| !d.pure && !d.dynamic) {
        diags.push(Diagnostic::at(
            Code::impure_in_pure,
            Some(id),
            None,
            format!(
                "`{}` has side effects, so a pure graph can't use it",
                def.op
            ),
        ));
    }
}

/// The rules of the graph's kind.
pub(super) fn kind(graph: &Graph, defs: &BTreeMap<&str, NodeDef>, diags: &mut Vec<Diagnostic>) {
    let count = |op: &str| defs.values().filter(|d| d.op == op).count();
    let rule = |message: String| Diagnostic::at(Code::kind_rule, None, None, message);
    match graph.kind {
        GraphKind::Event => {
            if !defs.values().any(|d| d.event && d.op != "behavior.root") {
                diags.push(rule(
                    "an event graph needs an event node (`event.on_tick` or `on.<Signal>`)".into(),
                ));
            }
        }
        GraphKind::Behavior => {
            let roots = count("behavior.root");
            if roots != 1 {
                diags.push(rule(format!(
                    "a behavior tree needs exactly one `behavior.root` (it has {roots})"
                )));
            }
        }
        GraphKind::Function | GraphKind::Utility => {
            if !graph.signature.outputs.is_empty() && count("graph.output") == 0 {
                diags.push(rule(
                    "a graph with outputs needs a `graph.output` node".into(),
                ));
            }
            if graph.kind == GraphKind::Utility {
                utility_scores(graph, defs, diags);
            }
        }
    }
}

/// A utility graph returns f32 scores, each straight from a node that keeps it in [0, 1].
fn utility_scores(graph: &Graph, defs: &BTreeMap<&str, NodeDef>, diags: &mut Vec<Diagnostic>) {
    let scores: Vec<&str> = graph
        .signature
        .outputs
        .iter()
        .filter(|o| parse_type(&o.ty, None, "").is_ok_and(|t| t == PinType::F32))
        .map(|o| o.name.as_str())
        .collect();
    if scores.is_empty() {
        diags.push(Diagnostic::at(
            Code::kind_rule,
            None,
            None,
            "a utility graph returns at least one f32 score".into(),
        ));
    }
    for id in defs
        .iter()
        .filter(|(_, d)| d.op == "graph.output")
        .map(|(id, _)| id)
    {
        for pin in &scores {
            let from = match graph.nodes[*id].inputs.get(*pin) {
                Some(Input::Link(src, _)) => defs.get(src.as_str()).map(|d| d.op.as_str()),
                _ => None,
            };
            if !from.is_some_and(|op| SCORE_OPS.contains(&op)) {
                let mut d = Diagnostic::at(
                    Code::kind_rule,
                    Some(id),
                    Some(pin),
                    format!(
                        "score `{pin}` must come straight from `utility.curve` or `math.saturate` (kept in [0, 1])"
                    ),
                );
                d.suggestion = Some("utility.curve".into());
                diags.push(d);
            }
        }
    }
}
