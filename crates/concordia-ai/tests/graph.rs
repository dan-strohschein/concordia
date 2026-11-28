//! Behavior graphs on the behavior-tree runtime (M4 4.12e, M10): a `.graph.ron` of kind Behavior
//! compiles to the same flat `ai.BehaviorTree` a `.bt.ron` holds.

use std::collections::BTreeMap;

use concordia_ai::{Abort, BbType, BbValue, CmpOp, NodeKind, TaskKind, compile_behavior};
use concordia_graph::{ExecLink, GraphKind, Node, NodeLibrary};

mod support;
use support::{guard, id, text};

#[test]
fn bt_graph_kind_compiles_to_bt_runtime() {
    let tree = compile_behavior(&guard(), &NodeLibrary::core()).unwrap();
    assert_eq!(
        tree.keys,
        BTreeMap::from([
            ("mood".into(), BbType::Text),
            ("target".into(), BbType::Entity),
            ("target_position".into(), BbType::Vec3)
        ])
    );
    // Selector [Condition(target set, lower) → MoveTo, Sequence [Wait 2, SetBlackboard mood]].
    assert!(matches!(tree.nodes[0].kind, NodeKind::Selector));
    assert_eq!(
        tree.nodes[1].kind,
        NodeKind::Condition {
            key: "target".into(),
            op: CmpOp::IsSet,
            value: BbValue::None,
            abort: Abort::Lower
        }
    );
    assert_eq!(
        tree.nodes[2].kind,
        NodeKind::Task(TaskKind::MoveTo {
            key: "target_position".into()
        })
    );
    assert_eq!(
        tree.nodes[5].kind,
        NodeKind::Task(TaskKind::SetBlackboard {
            key: "mood".into(),
            value: BbValue::Text("bored".into())
        })
    );
    insta::assert_json_snapshot!("guard_tree", tree);
}

#[test]
fn only_behavior_graphs_with_behavior_nodes_compile() {
    let lib = NodeLibrary::core();
    let mut wrong_kind = guard();
    wrong_kind.kind = GraphKind::Event;
    assert!(compile_behavior(&wrong_kind, &lib).is_err());
    // A data node can't be a tree node.
    let mut g = guard();
    g.nodes.insert(id(8), Node::new("entity.self"));
    g.exec
        .push(ExecLink(id(5), "child2".into(), id(8), "in".into()));
    let errors = compile_behavior(&g, &lib).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.node_id.as_deref() == Some(id(8).as_str())),
        "{errors:?}"
    );
    // A value that doesn't parse for its key names the node and pin.
    let mut g = guard();
    g.nodes
        .get_mut(&id(3))
        .unwrap()
        .inputs
        .insert("op".into(), text("roughly"));
    let errors = compile_behavior(&g, &lib).unwrap_err();
    assert!(
        errors.iter().any(|d| d.pin.as_deref() == Some("op")),
        "{errors:?}"
    );
}
