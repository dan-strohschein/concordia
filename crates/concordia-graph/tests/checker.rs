//! The type checker (M4 4.9, H7, H8): diagnostics name the node and pin to fix.

use std::collections::BTreeMap;

use concordia_graph::{
    Code, Diagnostic, ExecLink, Graph, GraphKind, Input, Literal, Node, NodeLibrary, PinDecl,
    PinType, Signature, Variable, check,
};
use concordia_reflect::{Reflect, TypeRegistry};
use serde::{Deserialize, Serialize};

/// Hit points.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
struct Health {
    /// Current.
    value: f32,
}

fn library() -> NodeLibrary {
    let mut reg = TypeRegistry::new();
    reg.register::<Health>().unwrap();
    NodeLibrary::build(&reg, |r| r.info().name == "Health")
}

fn id(n: u32) -> String {
    format!("0193b0f2-0000-7000-8000-{n:012x}")
}

fn graph(kind: GraphKind, nodes: Vec<(u32, Node)>, exec: Vec<(u32, &str, u32, &str)>) -> Graph {
    Graph {
        id: id(1),
        kind,
        version: 1,
        signature: Signature::default(),
        variables: vec![],
        nodes: nodes.into_iter().map(|(n, node)| (id(n), node)).collect(),
        exec: exec
            .into_iter()
            .map(|(a, p, b, q)| ExecLink(id(a), p.into(), id(b), q.into()))
            .collect(),
        layout: BTreeMap::new(),
    }
}

fn link(n: u32, pin: &str) -> Input {
    Input::Link(id(n), pin.into())
}

fn value(v: f64) -> Input {
    Input::Value(Literal::Float(v))
}

fn codes(d: &[Diagnostic]) -> Vec<Code> {
    d.iter().map(|d| d.code).collect()
}

/// On tick, compare the entity's health with 10 and branch.
fn clean() -> Graph {
    graph(
        GraphKind::Event,
        vec![
            (10, Node::new("event.on_tick")),
            (11, Node::new("get.Health.value")),
            (
                12,
                Node::new("math.lt")
                    .with_input("a", link(11, "value"))
                    .with_input("b", value(10.0)),
            ),
            (13, Node::new("branch").with_input("cond", link(12, "out"))),
        ],
        vec![(10, "out", 13, "in")],
    )
}

#[test]
fn a_well_typed_graph_checks_clean() {
    let report = check(&clean(), &library());
    assert_eq!(report.diagnostics, vec![]);
    assert_eq!(report.pin_type(&id(12), "out"), Some(&PinType::Bool));
}

#[test]
fn type_checker_rejects_type_mismatch_with_node_and_pin() {
    let mut g = clean();
    // `branch.cond` wants a bool; give it the f32 health.
    g.nodes
        .get_mut(&id(13))
        .unwrap()
        .inputs
        .insert("cond".into(), link(11, "value"));
    let d = check(&g, &library()).diagnostics;
    assert_eq!(codes(&d), vec![Code::type_mismatch]);
    assert_eq!(d[0].node_id.as_deref(), Some(id(13).as_str()));
    assert_eq!(d[0].pin.as_deref(), Some("cond"));
    assert!(
        d[0].message.contains("bool") && d[0].message.contains("f32"),
        "{}",
        d[0].message
    );
}

#[test]
fn type_checker_rejects_data_cycle() {
    let g = graph(
        GraphKind::Event,
        vec![
            (10, Node::new("event.on_tick")),
            (
                11,
                Node::new("math.add")
                    .with_input("a", link(12, "out"))
                    .with_input("b", value(1.0)),
            ),
            (
                12,
                Node::new("math.add")
                    .with_input("a", link(11, "out"))
                    .with_input("b", value(1.0)),
            ),
        ],
        vec![],
    );
    let d = check(&g, &library()).diagnostics;
    assert!(codes(&d).contains(&Code::data_cycle), "{d:?}");
}

#[test]
fn type_checker_rejects_unconnected_required_input() {
    let mut g = clean();
    g.nodes.get_mut(&id(12)).unwrap().inputs.remove("b");
    let d = check(&g, &library()).diagnostics;
    assert_eq!(codes(&d), vec![Code::unconnected_input]);
    assert_eq!(
        (d[0].node_id.as_deref(), d[0].pin.as_deref()),
        (Some(id(12).as_str()), Some("b"))
    );
}

#[test]
fn type_checker_rejects_impure_node_in_pure_function() {
    let mut g = graph(
        GraphKind::Function,
        vec![
            (10, Node::new("graph.input")),
            (
                11,
                Node::new("set.Health")
                    .with_input("value", Input::Value(Literal::Text("x".into()))),
            ),
            (12, Node::new("graph.output").with_input("y", link(10, "x"))),
        ],
        vec![],
    );
    g.signature = Signature {
        inputs: vec![PinDecl {
            name: "x".into(),
            ty: "f32".into(),
        }],
        outputs: vec![PinDecl {
            name: "y".into(),
            ty: "f32".into(),
        }],
        pure: true,
    };
    let d = check(&g, &library()).diagnostics;
    assert!(
        d.iter().any(
            |d| d.code == Code::impure_in_pure && d.node_id.as_deref() == Some(id(11).as_str())
        ),
        "{d:?}"
    );
}

#[test]
fn type_checker_monomorphizes_numeric_generics() {
    // Vec3 + Vec3 is a Vec3; its length is an f32 compared with 1.
    let g = graph(
        GraphKind::Event,
        vec![
            (10, Node::new("event.on_tick")),
            (
                11,
                Node::new("math.add")
                    .with_input("a", Input::Value(Literal::Vec3([1.0, 0.0, 0.0])))
                    .with_input("b", Input::Value(Literal::Vec3([0.0, 1.0, 0.0]))),
            ),
            (
                12,
                Node::new("vector.length").with_input("v", link(11, "out")),
            ),
            (
                13,
                Node::new("math.gt")
                    .with_input("a", link(12, "out"))
                    .with_input("b", value(1.0)),
            ),
            (14, Node::new("branch").with_input("cond", link(13, "out"))),
        ],
        vec![(10, "out", 14, "in")],
    );
    let report = check(&g, &library());
    assert_eq!(report.diagnostics, vec![]);
    assert_eq!(report.pin_type(&id(11), "out"), Some(&PinType::Vec3));
    // Vec3 + f32 doesn't resolve.
    let mut bad = g.clone();
    bad.nodes
        .get_mut(&id(11))
        .unwrap()
        .inputs
        .insert("b".into(), value(2.0));
    let d = check(&bad, &library()).diagnostics;
    assert!(d.iter().any(|d| d.code == Code::type_mismatch && d.node_id.as_deref() == Some(id(11).as_str())), "{d:?}");
    // A bool isn't Numeric.
    let mut bad = g;
    bad.nodes.get_mut(&id(11)).unwrap().inputs = BTreeMap::from([
        ("a".into(), Input::Value(Literal::Bool(true))),
        ("b".into(), Input::Value(Literal::Bool(false))),
    ]);
    let d = check(&bad, &library()).diagnostics;
    assert!(
        d.iter().any(|d| d.code == Code::generic_unresolved),
        "{d:?}"
    );
}

#[test]
fn type_checker_rejects_unknown_op_with_closest_suggestion() {
    let mut g = clean();
    g.nodes.get_mut(&id(12)).unwrap().op = "math.lte".into();
    let d = check(&g, &library()).diagnostics;
    assert_eq!(codes(&d), vec![Code::unknown_op]);
    let s = d[0].suggestion.as_deref().unwrap_or_default();
    assert!(s.contains("math.lt") || s.contains("math.le"), "{s}");
}

#[test]
fn variables_are_typed_by_their_declarations() {
    let mut g = clean();
    g.variables = vec![Variable {
        name: "rage".into(),
        ty: "f32".into(),
        default: None,
    }];
    g.nodes.insert(
        id(14),
        Node::new("var.set.rage").with_input("value", link(11, "value")),
    );
    g.exec
        .push(ExecLink(id(13), "true".into(), id(14), "in".into()));
    assert_eq!(check(&g, &library()).diagnostics, vec![]);
    g.nodes.get_mut(&id(14)).unwrap().op = "var.set.rag".into();
    let d = check(&g, &library()).diagnostics;
    assert_eq!(codes(&d), vec![Code::unknown_variable]);
    assert_eq!(d[0].suggestion.as_deref(), Some("var.set.rage"));
}

#[test]
fn diagnostics_are_valid_json_schema() {
    let schema = concordia_reflect::schema_for(Diagnostic::type_info());
    jsonschema::draft202012::meta::validate(&schema).expect("a valid draft 2020-12 schema");
    let validator = jsonschema::draft202012::new(&schema).unwrap();
    let mut g = clean();
    g.nodes.get_mut(&id(12)).unwrap().op = "math.lte".into();
    for d in check(&g, &library()).diagnostics {
        let json = serde_json::to_value(&d).unwrap();
        assert!(validator.is_valid(&json), "{json}");
    }
}

#[test]
fn graph_kind_rules_enforced() {
    let lib = library();
    // An event graph needs an event.
    let no_event = graph(
        GraphKind::Event,
        vec![(11, Node::new("entity.self"))],
        vec![],
    );
    assert!(codes(&check(&no_event, &lib).diagnostics).contains(&Code::kind_rule));
    // A behavior tree needs exactly one root.
    let two_roots = graph(
        GraphKind::Behavior,
        vec![
            (10, Node::new("behavior.root")),
            (11, Node::new("behavior.root")),
        ],
        vec![],
    );
    assert!(codes(&check(&two_roots, &lib).diagnostics).contains(&Code::kind_rule));
    let one_root = graph(
        GraphKind::Behavior,
        vec![
            (10, Node::new("behavior.root")),
            (11, Node::new("behavior.succeed")),
        ],
        vec![(10, "child", 11, "in")],
    );
    assert_eq!(check(&one_root, &lib).diagnostics, vec![]);
    // A utility consideration's score comes from a node that keeps it in [0, 1].
    let mut utility = graph(
        GraphKind::Utility,
        vec![
            (10, Node::new("graph.input")),
            (
                11,
                Node::new("math.mul")
                    .with_input("a", link(10, "distance"))
                    .with_input("b", value(0.1)),
            ),
            (
                12,
                Node::new("graph.output").with_input("score", link(11, "out")),
            ),
        ],
        vec![],
    );
    utility.signature = Signature {
        inputs: vec![PinDecl {
            name: "distance".into(),
            ty: "f32".into(),
        }],
        outputs: vec![PinDecl {
            name: "score".into(),
            ty: "f32".into(),
        }],
        pure: true,
    };
    assert!(codes(&check(&utility, &lib).diagnostics).contains(&Code::kind_rule));
    utility.nodes.insert(
        id(13),
        Node::new("utility.curve").with_input("x", link(11, "out")),
    );
    utility
        .nodes
        .get_mut(&id(12))
        .unwrap()
        .inputs
        .insert("score".into(), link(13, "out"));
    assert_eq!(check(&utility, &lib).diagnostics, vec![]);
}
