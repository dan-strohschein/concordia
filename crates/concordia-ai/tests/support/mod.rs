//! Shared test fixtures: the guard Behavior graph.

use std::collections::BTreeMap;

use concordia_graph::{ExecLink, Graph, GraphKind, Input, Literal, Node, Signature, Variable};

pub fn id(n: u32) -> String {
    format!("0193b0f2-0000-7000-8000-{n:012x}")
}

pub fn text(v: &str) -> Input {
    Input::Value(Literal::Text(v.into()))
}

/// A guard: chase what it perceives (interrupting a patrol), else patrol: wait, then turn.
pub fn guard() -> Graph {
    let nodes = [
        (1, Node::new("behavior.root")),
        (2, Node::new("behavior.selector")),
        (
            3,
            Node::new("behavior.condition")
                .with_input("key", text("target"))
                .with_input("op", text("set"))
                .with_input("abort", text("lower")),
        ),
        (
            4,
            Node::new("behavior.move_to").with_input("key", text("target_position")),
        ),
        (5, Node::new("behavior.sequence")),
        (
            6,
            Node::new("behavior.wait").with_input("seconds", Input::Value(Literal::Float(2.0))),
        ),
        (
            7,
            Node::new("behavior.set_key")
                .with_input("key", text("mood"))
                .with_input("value", text("bored")),
        ),
    ];
    let link = |a: u32, p: &str, b: u32| ExecLink(id(a), p.into(), id(b), "in".into());
    Graph {
        id: id(100),
        kind: GraphKind::Behavior,
        version: 1,
        signature: Signature::default(),
        variables: vec![
            Variable {
                name: "target".into(),
                ty: "EntityId".into(),
                default: None,
            },
            Variable {
                name: "target_position".into(),
                ty: "Vec3".into(),
                default: None,
            },
            Variable {
                name: "mood".into(),
                ty: "String".into(),
                default: None,
            },
        ],
        nodes: nodes.into_iter().map(|(n, node)| (id(n), node)).collect(),
        exec: vec![
            link(1, "child", 2),
            link(2, "child0", 3),
            link(3, "child", 4),
            link(2, "child1", 5),
            link(5, "child0", 6),
            link(5, "child1", 7),
        ],
        layout: BTreeMap::new(),
    }
}
