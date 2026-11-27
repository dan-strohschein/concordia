//! Structured graph edits (M4 4.10c, J18): what `graph.apply` does to a graph, all or nothing.

use concordia_graph::{
    GraphEdit, GraphKind, Input, Literal, NodeLibrary, apply_edits, check, new_graph, write_graph,
};

fn id(n: u32) -> String {
    format!("0193b0f2-0000-7000-8000-{n:012x}")
}

/// An agent's session: on tick, if dt > 0.01, raise the counter variable by one.
fn authored() -> Vec<GraphEdit> {
    let node = |n: u32, op: &str| GraphEdit::AddNode {
        id: Some(id(n)),
        op: op.into(),
        at: Some((n as f32 * 100.0, 0.0)),
    };
    vec![
        GraphEdit::SetVariable {
            name: "count".into(),
            ty: "i32".into(),
            default: Some(Literal::Int(0)),
        },
        node(1, "event.on_tick"),
        node(2, "math.gt"),
        node(3, "branch"),
        node(4, "var.get.count"),
        node(5, "math.add"),
        node(6, "var.set.count"),
        GraphEdit::Connect {
            from: id(1),
            from_pin: "dt".into(),
            to: id(2),
            to_pin: "a".into(),
        },
        GraphEdit::SetValue {
            node: id(2),
            pin: "b".into(),
            value: Literal::Float(0.01),
        },
        GraphEdit::Connect {
            from: id(2),
            from_pin: "out".into(),
            to: id(3),
            to_pin: "cond".into(),
        },
        GraphEdit::Connect {
            from: id(4),
            from_pin: "value".into(),
            to: id(5),
            to_pin: "a".into(),
        },
        GraphEdit::SetValue {
            node: id(5),
            pin: "b".into(),
            value: Literal::Int(1),
        },
        GraphEdit::Connect {
            from: id(5),
            from_pin: "out".into(),
            to: id(6),
            to_pin: "value".into(),
        },
        GraphEdit::ConnectExec {
            from: id(1),
            from_pin: "out".into(),
            to: id(3),
            to_pin: "in".into(),
        },
        GraphEdit::ConnectExec {
            from: id(3),
            from_pin: "true".into(),
            to: id(6),
            to_pin: "in".into(),
        },
    ]
}

#[test]
fn edits_build_a_graph_that_checks_clean() {
    let mut g = new_graph(GraphKind::Event, id(100));
    let added = apply_edits(&mut g, &authored()).unwrap();
    assert_eq!(added.len(), 6);
    assert_eq!(check(&g, &NodeLibrary::core()).diagnostics, vec![]);
    assert_eq!(g.layout[&id(3)], (300.0, 0.0));
    write_graph(&g).unwrap();
}

#[test]
fn removing_a_node_removes_its_links() {
    let mut g = new_graph(GraphKind::Event, id(100));
    apply_edits(&mut g, &authored()).unwrap();
    apply_edits(&mut g, &[GraphEdit::RemoveNode { id: id(3) }]).unwrap();
    assert!(!g.nodes.contains_key(&id(3)) && !g.layout.contains_key(&id(3)));
    assert!(
        g.exec.iter().all(|e| e.0 != id(3) && e.2 != id(3)),
        "{:?}",
        g.exec
    );
    assert!(g.nodes.values().all(|n| {
        n.inputs
            .values()
            .all(|i| !matches!(i, Input::Link(src, _) if *src == id(3)))
    }));
}

#[test]
fn disconnecting_and_unsetting() {
    let mut g = new_graph(GraphKind::Event, id(100));
    apply_edits(&mut g, &authored()).unwrap();
    apply_edits(
        &mut g,
        &[
            GraphEdit::Disconnect {
                to: id(3),
                to_pin: "cond".into(),
            },
            GraphEdit::DisconnectExec {
                from: id(3),
                from_pin: "true".into(),
                to: id(6),
                to_pin: "in".into(),
            },
            GraphEdit::RemoveVariable {
                name: "count".into(),
            },
        ],
    )
    .unwrap();
    assert!(!g.nodes[&id(3)].inputs.contains_key("cond"));
    assert_eq!(g.exec.len(), 1);
    assert!(g.variables.is_empty());
}

#[test]
fn a_bad_edit_changes_nothing_and_says_which() {
    let mut g = new_graph(GraphKind::Event, id(100));
    apply_edits(&mut g, &authored()).unwrap();
    let before = g.clone();
    let err = apply_edits(
        &mut g,
        &[
            GraphEdit::SetValue {
                node: id(2),
                pin: "b".into(),
                value: Literal::Float(9.0),
            },
            GraphEdit::Connect {
                from: id(77),
                from_pin: "out".into(),
                to: id(3),
                to_pin: "cond".into(),
            },
        ],
    )
    .unwrap_err();
    assert_eq!(err.index, 1);
    assert!(err.message.contains(&id(77)), "{}", err.message);
    assert_eq!(g, before, "all or nothing");
    // Ids must be UUIDv7 and new.
    let err = apply_edits(
        &mut g,
        &[GraphEdit::AddNode {
            id: Some("node-1".into()),
            op: "branch".into(),
            at: None,
        }],
    )
    .unwrap_err();
    assert!(err.message.contains("UUIDv7"), "{}", err.message);
    let err = apply_edits(
        &mut g,
        &[GraphEdit::AddNode {
            id: Some(id(3)),
            op: "branch".into(),
            at: None,
        }],
    )
    .unwrap_err();
    assert!(err.message.contains("already"), "{}", err.message);
}

#[test]
fn added_nodes_without_ids_get_fresh_uuid_v7s() {
    let mut g = new_graph(GraphKind::Event, id(100));
    let added = apply_edits(
        &mut g,
        &[GraphEdit::AddNode {
            id: None,
            op: "event.on_tick".into(),
            at: None,
        }],
    )
    .unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(
        uuid::Uuid::parse_str(&added[0]).unwrap().get_version_num(),
        7
    );
    write_graph(&g).unwrap();
}

#[test]
fn moving_a_node_changes_only_the_layout() {
    let mut g = new_graph(GraphKind::Event, id(100));
    apply_edits(&mut g, &authored()).unwrap();
    let hash = g.semantic_hash().unwrap();
    apply_edits(
        &mut g,
        &[GraphEdit::SetPosition {
            node: id(3),
            at: (12.0, 34.0),
        }],
    )
    .unwrap();
    assert_eq!(g.layout[&id(3)], (12.0, 34.0));
    assert_eq!(g.semantic_hash().unwrap(), hash, "cosmetic");
    assert!(
        apply_edits(
            &mut g,
            &[GraphEdit::SetPosition {
                node: id(99),
                at: (0.0, 0.0)
            }]
        )
        .is_err()
    );
}
