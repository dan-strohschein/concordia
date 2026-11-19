//! Graphviz output of a compiled graph (readable by agents and humans).

use super::{CompiledGraph, RenderGraph, Use};

impl CompiledGraph {
    /// A `digraph`: passes in execution order, textures as boxes, writes and reads as edges.
    pub fn to_dot(&self, graph: &RenderGraph) -> String {
        let mut out = String::from("digraph frame {\n  rankdir=LR;\n");
        for (pos, &p) in self.order().iter().enumerate() {
            out.push_str(&format!(
                "  p{p} [label=\"{pos}: {}\" shape=ellipse];\n",
                graph.passes[p].name
            ));
        }
        let mut used: Vec<u32> = self
            .order()
            .iter()
            .flat_map(|&p| graph.passes[p].uses.iter().map(|(t, _)| t.0))
            .collect();
        used.sort_unstable();
        used.dedup();
        for t in used {
            out.push_str(&format!(
                "  t{t} [label=\"{}\" shape=box];\n",
                graph.resources[t as usize].desc.label
            ));
        }
        for &p in self.order() {
            for (t, how) in &graph.passes[p].uses {
                let edge = match how.is_write() {
                    true => format!("  p{p} -> t{} [label=\"{}\"];\n", t.0, name(*how)),
                    false => format!("  t{} -> p{p} [label=\"{}\"];\n", t.0, name(*how)),
                };
                out.push_str(&edge);
            }
        }
        out.push_str("}\n");
        out
    }
}

fn name(how: Use) -> &'static str {
    match how {
        Use::ColorWrite => "color",
        Use::DepthWrite => "depth write",
        Use::DepthRead => "depth read",
        Use::SampledRead => "sampled",
        Use::StorageRead => "storage read",
        Use::StorageWrite => "storage write",
        Use::CopySrc => "copy src",
        Use::CopyDst => "copy dst",
    }
}
