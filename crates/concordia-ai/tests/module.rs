//! The AI engine module (M4 4.12b, M1, M11): brains load their `.bt.ron` trees when play starts,
//! and agents inspect and trace them over the Engine API.

use std::collections::BTreeMap;
use std::sync::Arc;

use concordia_ai::{AiModule, BehaviorTree, NodeKind, Spec, TaskKind};
use concordia_api::{Engine, EngineConfig};
use serde_json::{Value, json};

fn fidget() -> BehaviorTree {
    let wait = |s| Spec::new(NodeKind::Task(TaskKind::Wait { seconds: s }));
    BehaviorTree::build(
        BTreeMap::new(),
        Spec::new(NodeKind::Loop { count: 0 }).children(vec![
            Spec::new(NodeKind::RandomSelector {
                weights: vec![1.0, 1.0],
            })
            .children(vec![wait(0.1), wait(0.2)]),
        ]),
    )
    .unwrap()
}

struct Project {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: concordia_core::id::SessionId,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ai")).unwrap();
        std::fs::write(
            dir.path().join("ai/fidget.bt.ron"),
            concordia_scene::write_document(&fidget()).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("ai/broken.bt.ron"),
            "BehaviorTree(nodes: [oops",
        )
        .unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(7);
        config.journal.sync = false;
        config.modules = vec![Arc::new(AiModule::default())];
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut p = Self {
            _dir: dir,
            engine,
            session,
        };
        p.ok(
            "session.open",
            json!({"client": "test", "actor": "agent:test"}),
        );
        p.ok(
            "scene.new",
            json!({"path": "scenes/a.scene.ron", "name": "A"}),
        );
        p
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }
}

#[test]
fn brains_load_their_trees_and_think_in_play() {
    let mut p = Project::new();
    let e = p.ok(
        "entity.spawn",
        json!({"components": {"ai.Brain": {"tree": "ai/fidget.bt.ron"}}}),
    )["id"]
        .clone();
    p.ok("sim.step", json!({"ticks": 5}));
    let inspected = p.ok("ai.inspect", json!({"entity": e}));
    assert_eq!(inspected["tree"], "ai/fidget.bt.ron");
    let running: Vec<String> = inspected["running"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["kind"].as_str().unwrap().to_owned())
        .collect();
    assert!(running.contains(&"Wait".to_owned()), "{inspected}");
    assert!(inspected["blackboard"].is_object());
}

#[test]
fn ai_trace_reports_decisions_per_tick() {
    let mut p = Project::new();
    let e = p.ok(
        "entity.spawn",
        json!({"components": {"ai.Brain": {"tree": "ai/fidget.bt.ron"}}}),
    )["id"]
        .clone();
    let traced = p.ok("ai.trace", json!({"entity": e, "ticks": 30}));
    let ticks = traced["ticks"].as_array().unwrap();
    assert_eq!(ticks.len(), 30);
    let picks = ticks
        .iter()
        .flat_map(|t| t["decisions"].as_array().unwrap().iter())
        .filter(|d| d["reason"] == "random")
        .count();
    // Half a second of waits of 0.1 or 0.2 s (each loop iteration takes one more tick): 2 to 5 picks.
    assert!((2..=5).contains(&picks), "{picks} picks: {traced}");
}

#[test]
fn a_broken_tree_is_reported_and_play_still_starts() {
    let mut p = Project::new();
    let e = p.ok(
        "entity.spawn",
        json!({"components": {"ai.Brain": {"tree": "ai/broken.bt.ron"}}}),
    )["id"]
        .clone();
    p.ok("sim.step", json!({"ticks": 2}));
    let err = p.call("ai.inspect", json!({"entity": e})).unwrap_err();
    assert!(err.message.contains("ai/broken.bt.ron"), "{}", err.message);
}

/// A guard at the origin facing +Z, a thief 8 m ahead, and (optionally) a wall between them.
fn sight_through_physics(wall: bool) -> Value {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("ai")).unwrap();
    let tree = BehaviorTree::build(
        BTreeMap::new(),
        Spec::new(NodeKind::Task(TaskKind::Wait { seconds: 1e6 })),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("ai/idle.bt.ron"),
        concordia_scene::write_document(&tree).unwrap(),
    )
    .unwrap();
    let mut config = EngineConfig::new(dir.path());
    config.deterministic_ids = Some(7);
    config.journal.sync = false;
    config.modules = vec![
        Arc::new(concordia_physics::PhysicsModule::default()),
        Arc::new(AiModule::default()),
    ];
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    let mut ok = |m: &str, p: Value| {
        engine
            .call(s, m, p.clone())
            .unwrap_or_else(|e| panic!("{m} {p}: {e:?}"))
    };
    ok(
        "session.open",
        json!({"client": "test", "actor": "agent:test"}),
    );
    ok(
        "scene.new",
        json!({"path": "scenes/a.scene.ron", "name": "A"}),
    );
    let at = |z: f32| json!({"translation": [0.0, 0.0, z], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]});
    let guard = ok("entity.spawn", json!({"components": {
        "Transform": at(0.0), "ai.Brain": {"tree": "ai/idle.bt.ron"},
        "ai.Perceiver": {"sight_range": 20.0, "sight_fov": 90.0, "hearing_range": 0.0, "memory_seconds": 5.0, "eye_height": 1.0, "notices": []},
        "ai.Faction": {"name": "guards"}}}))["id"].clone();
    let thief = ok(
        "entity.spawn",
        json!({"components": {"Transform": at(8.0), "ai.Faction": {"name": "thieves"}}}),
    )["id"]
        .clone();
    if wall {
        ok(
            "entity.spawn",
            json!({"components": {
            "Transform": {"translation": [0.0, 1.0, 4.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
            "RigidBody": {"kind": "Static"},
            "Collider": {"shape": {"Box": {"half_extents": [3.0, 2.0, 0.2]}}}}}),
        );
    }
    ok("sim.step", json!({"ticks": 3}));
    let memory = ok(
        "component.get",
        json!({"entity": guard, "component": "ai.Memory", "world": "play"}),
    )["value"]
        .clone();
    json!({"thief": thief, "memory": memory})
}

#[test]
fn perception_sight_respects_cone_range_and_occlusion() {
    let open = sight_through_physics(false);
    let seen = open["memory"]["entries"].as_array().unwrap();
    assert!(
        seen.iter()
            .any(|e| e["entity"] == open["thief"] && e["sense"] == "Sight"),
        "{open}"
    );
    let walled = sight_through_physics(true);
    assert_eq!(
        walled["memory"]["entries"],
        json!([]),
        "the wall hides the thief: {walled}"
    );
}
