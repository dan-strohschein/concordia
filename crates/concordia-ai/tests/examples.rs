//! The phase's done criterion (M4 4.12e): behavior-tree, utility and perception examples run under
//! replay, the tree authored both as a Behavior graph and as a `.bt.ron`.

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;

use concordia_ai::{
    AiModule, BbType, BehaviorTree, Consideration, Curve, Input, NodeKind, Selection, Spec,
    TaskKind, UtilityOption,
};
use concordia_api::{Engine, EngineConfig};
use concordia_graph::{ExecLink, Literal, Node, write_graph};
use serde_json::{Value, json};

/// The guard: once it perceives an intruder, face it and cast; else wait.
fn guard_graph() -> concordia_graph::Graph {
    let mut g = support::guard();
    let id = |n: u32| format!("0193b0f2-0000-7000-8000-{n:012x}");
    let text = |v: &str| concordia_graph::Input::Value(Literal::Text(v.into()));
    // Replace move_to (no navigation here) with face-then-cast.
    g.nodes.insert(id(4), Node::new("behavior.sequence"));
    g.nodes.insert(
        id(8),
        Node::new("behavior.face").with_input("key", text("target")),
    );
    g.nodes.insert(
        id(9),
        Node::new("behavior.cast")
            .with_input("skill", text("zap"))
            .with_input("target_key", text("target")),
    );
    g.exec
        .push(ExecLink(id(4), "child0".into(), id(8), "in".into()));
    g.exec
        .push(ExecLink(id(4), "child1".into(), id(9), "in".into()));
    g
}

/// The scout: rest when tired, patrol when rested (utility on `energy`), forever.
fn scout_tree() -> BehaviorTree {
    let keys = BTreeMap::from([("energy".into(), BbType::Float)]);
    let option = |name: &str, slope: f32, intercept: f32| UtilityOption {
        name: name.into(),
        weight: 1.0,
        considerations: vec![Consideration {
            input: Input::Key("energy".into()),
            curve: Curve::Linear { slope, intercept },
        }],
    };
    let set = |v: f32| {
        Spec::new(NodeKind::Task(TaskKind::SetBlackboard {
            key: "energy".into(),
            value: concordia_ai::BbValue::Float(v),
        }))
    };
    let wait = |s: f32| Spec::new(NodeKind::Task(TaskKind::Wait { seconds: s }));
    BehaviorTree::build(
        keys,
        Spec::new(NodeKind::Loop { count: 0 }).children(vec![
            Spec::new(NodeKind::UtilitySelector {
                options: vec![option("rest", -1.0, 1.0), option("patrol", 1.0, 0.0)],
                selection: Selection::Best { momentum: 0.0 },
            })
            .children(vec![
                Spec::new(NodeKind::Sequence).children(vec![wait(0.25), set(1.0)]),
                Spec::new(NodeKind::Sequence).children(vec![wait(0.25), set(0.1)]),
            ]),
        ]),
    )
    .unwrap()
}

fn engine(dir: &std::path::Path) -> (Engine, concordia_core::id::SessionId) {
    let mut config = EngineConfig::new(dir);
    config.deterministic_ids = Some(7);
    config.journal.sync = false;
    config.modules = vec![
        Arc::new(concordia_physics::PhysicsModule::default()),
        Arc::new(concordia_replay::ReplayModule::new()),
        Arc::new(AiModule::default()),
    ];
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    engine
        .call(
            s,
            "session.open",
            json!({"client": "test", "actor": "agent:test"}),
        )
        .unwrap();
    (engine, s)
}

#[test]
fn behavior_utility_and_perception_examples_run_under_replay() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("ai")).unwrap();
    std::fs::write(
        dir.path().join("ai/guard.graph.ron"),
        write_graph(&guard_graph()).unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("ai/scout.bt.ron"),
        concordia_scene::write_document(&scout_tree()).unwrap(),
    )
    .unwrap();
    let (mut live, s) = engine(dir.path());
    let mut ok = |m: &str, p: Value| {
        live.call(s, m, p.clone())
            .unwrap_or_else(|e| panic!("{m} {p}: {e:?}"))
    };
    ok(
        "scene.new",
        json!({"path": "scenes/ai.scene.ron", "name": "AI"}),
    );
    let at = |x: f32, z: f32| json!({"translation": [x, 0.0, z], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]});
    ok(
        "entity.spawn",
        json!({"components": {"Name": "Guard", "Transform": at(0.0, 0.0), "ai.Brain": {"tree": "ai/guard.graph.ron"},
        "ai.Perceiver": {"sight_range": 12.0, "sight_fov": 100.0, "hearing_range": 10.0, "memory_seconds": 3.0, "eye_height": 1.0, "notices": []},
        "ai.Faction": {"name": "guards"}}}),
    );
    ok(
        "entity.spawn",
        json!({"components": {"Name": "Thief", "Transform": at(3.0, 6.0), "ai.Faction": {"name": "thieves"}}}),
    );
    ok(
        "entity.spawn",
        json!({"components": {"Name": "Scout", "Transform": at(-5.0, 0.0), "ai.Brain": {"tree": "ai/scout.bt.ron"}}}),
    );
    ok("scene.save", json!({}));
    ok(
        "replay.record",
        json!({"path": "tests/replays/ai.replay.ron"}),
    );
    ok("sim.step", json!({"ticks": 120}));
    let recorded = ok("replay.record", json!({"action": "stop"}));
    assert_eq!(recorded["tick"], 120, "{recorded}");
    drop(live);

    let (mut again, s) = engine(dir.path());
    let mut ok = |m: &str, p: Value| {
        again
            .call(s, m, p.clone())
            .unwrap_or_else(|e| panic!("{m} {p}: {e:?}"))
    };
    ok("scene.open", json!({"path": "scenes/ai.scene.ron"}));
    let run = ok("replay.run", json!({"path": "tests/replays/ai.replay.ron"}));
    assert_eq!(run["ok"], true, "the AI replays tick for tick: {run}");
    // The behaviors are assertable over the replay: the guard saw the thief and cast at it, and
    // the scout chose between resting and patrolling more than once.
    let checked = ok(
        "assert.eval",
        json!({"source": "replay", "path": "tests/replays/ai.replay.ron", "assertions": [
            "within 30 ticks: event(\"ai.Cast\")",
            "within 60 ticks: event(\"ai.Decision\")",
        ]}),
    );
    assert_eq!(checked["ok"], true, "{checked}");
}
