//! The `.graph.ron` format (M4 4.9): canonical round trips, the semantic hash, readable errors.

use std::collections::BTreeMap;

use concordia_graph::{
    ExecLink, Graph, GraphKind, Input, Literal, Node, PinDecl, Signature, Variable, read_graph,
    write_graph,
};
use proptest::prelude::*;

const G: &str = "0193b0f2-0000-7000-8000-000000000001";

fn id(n: u32) -> String {
    format!("0193b0f2-0000-7000-8000-{n:012x}")
}

/// A small event graph: on tick, if the distance exceeds 2, set the speed.
fn sample() -> Graph {
    let mut nodes = BTreeMap::new();
    nodes.insert(id(10), Node::new("event.on_tick"));
    nodes.insert(
        id(11),
        Node::new("math.gt.f32")
            .with_input("a", Input::Link(id(12), "out".into()))
            .with_input("b", Input::Value(Literal::Float(2.0))),
    );
    nodes.insert(id(12), Node::new("var.get.distance"));
    nodes.insert(
        id(13),
        Node::new("branch").with_input("cond", Input::Link(id(11), "out".into())),
    );
    nodes.insert(
        id(14),
        Node::new("var.set.speed").with_input("value", Input::Value(Literal::Float(3.5))),
    );
    Graph {
        id: G.into(),
        kind: GraphKind::Event,
        version: 1,
        signature: Signature::default(),
        variables: vec![
            Variable {
                name: "distance".into(),
                ty: "f32".into(),
                default: Some(Literal::Float(0.0)),
            },
            Variable {
                name: "speed".into(),
                ty: "f32".into(),
                default: None,
            },
        ],
        nodes,
        // Deliberately out of order: the printer sorts.
        exec: vec![
            ExecLink(id(13), "true".into(), id(14), "in".into()),
            ExecLink(id(10), "out".into(), id(13), "in".into()),
        ],
        layout: BTreeMap::from([(id(10), (0.0, 0.0)), (id(13), (240.0, 0.0))]),
    }
}

#[test]
fn graph_ron_roundtrips_canonically() {
    let text = write_graph(&sample()).unwrap();
    insta::assert_snapshot!("sample_event_graph", text);
    let back = read_graph(&text).unwrap();
    assert_eq!(back, sample().normalized());
    assert_eq!(write_graph(&back).unwrap(), text, "printing is idempotent");
}

#[test]
fn layout_changes_do_not_change_semantic_hash() {
    let a = sample();
    let mut b = sample();
    b.layout.insert(id(14), (480.0, 120.0));
    b.layout.insert(id(10), (-50.0, 10.0));
    assert_eq!(a.semantic_hash().unwrap(), b.semantic_hash().unwrap());
    // Exec order as written doesn't matter either; a semantic change does.
    let mut c = sample();
    c.exec.reverse();
    assert_eq!(a.semantic_hash().unwrap(), c.semantic_hash().unwrap());
    let mut d = sample();
    d.nodes
        .get_mut(&id(14))
        .unwrap()
        .inputs
        .insert("value".into(), Input::Value(Literal::Float(4.0)));
    assert_ne!(a.semantic_hash().unwrap(), d.semantic_hash().unwrap());
}

#[test]
fn bad_text_reports_where() {
    let text = write_graph(&sample())
        .unwrap()
        .replace("kind: Event", "kind: Eventt");
    let err = read_graph(&text).unwrap_err().to_string();
    assert!(err.contains("kind") || err.contains("Eventt"), "{err}");
    let err = read_graph("Graph(id: \"not-a-uuid\")")
        .unwrap_err()
        .to_string();
    assert!(!err.is_empty());
}

#[test]
fn ids_must_be_uuid_v7() {
    let mut g = sample();
    g.id = "0193b0f2-0000-4000-8000-000000000001".into();
    assert!(write_graph(&g).is_err(), "a v4 graph id");
    let mut g = sample();
    g.nodes.insert("node-a".into(), Node::new("branch"));
    assert!(write_graph(&g).is_err(), "a node id that isn't a UUID");
}

fn literal() -> impl Strategy<Value = Literal> {
    prop_oneof![
        any::<bool>().prop_map(Literal::Bool),
        (-1000i64..1000).prop_map(Literal::Int),
        (-1.0e6f64..1.0e6).prop_map(Literal::Float),
        "[a-z ]{0,8}".prop_map(Literal::Text),
        prop::array::uniform3(-100.0f32..100.0).prop_map(Literal::Vec3),
    ]
}

fn graph() -> impl Strategy<Value = Graph> {
    (
        prop::collection::btree_map(
            1u32..40,
            (
                "[a-z_.]{1,12}",
                prop::collection::btree_map(
                    "[a-z]{1,4}",
                    (any::<bool>(), 1u32..40, literal()),
                    0..3,
                ),
            ),
            0..8,
        ),
        prop::collection::vec((1u32..40, "[a-z]{1,4}", 1u32..40, "[a-z]{1,4}"), 0..6),
        prop::collection::btree_map(1u32..40, (-500.0f32..500.0, -500.0f32..500.0), 0..4),
        prop_oneof![
            Just(GraphKind::Event),
            Just(GraphKind::Function),
            Just(GraphKind::Behavior),
            Just(GraphKind::Utility)
        ],
    )
        .prop_map(|(nodes, exec, layout, kind)| Graph {
            id: G.into(),
            kind,
            version: 1,
            signature: Signature {
                inputs: vec![PinDecl {
                    name: "x".into(),
                    ty: "f32".into(),
                }],
                outputs: vec![],
                pure: false,
            },
            variables: vec![],
            nodes: nodes
                .into_iter()
                .map(|(n, (op, inputs))| {
                    let mut node = Node::new(&op);
                    for (pin, (link, to, lit)) in inputs {
                        let input = match link {
                            true => Input::Link(id(to), "out".into()),
                            false => Input::Value(lit),
                        };
                        node = node.with_input(&pin, input);
                    }
                    (id(n), node)
                })
                .collect(),
            exec: exec
                .into_iter()
                .map(|(a, p, b, q)| ExecLink(id(a), p, id(b), q))
                .collect(),
            layout: layout.into_iter().map(|(n, xy)| (id(n), xy)).collect(),
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn printing_then_parsing_gives_the_normalized_graph(g in graph()) {
        let text = write_graph(&g).unwrap();
        let back = read_graph(&text).unwrap();
        prop_assert_eq!(&back, &g.normalized());
        prop_assert_eq!(write_graph(&back).unwrap(), text);
    }
}
