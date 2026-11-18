//! The physics engine module: play-world stepping, `physics.raycast`, edits between sessions (M1 phase 1.7b).

use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_api::{Engine, EngineConfig, params_schema};
use concordia_core::id::SessionId;
use concordia_physics::{PhysicsModule, PhysicsSettings};
use serde_json::{Value, json};

struct Rig {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(3);
        config.threads = NonZeroUsize::MIN;
        config.journal.sync = false;
        config.modules = vec![Arc::new(PhysicsModule::new(PhysicsSettings::default()))];
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut rig = Self {
            _dir: dir,
            engine,
            session,
        };
        rig.ok(
            "session.open",
            json!({"client": "test", "actor": "agent:test"}),
        );
        rig.ok(
            "scene.new",
            json!({"path": "scenes/p.scene.ron", "name": "P"}),
        );
        rig
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    fn spawn(&mut self, y: f32, kind: &str, collider: Value) -> String {
        let components = json!({
            "Transform": {"translation": [0.0, y, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
            "RigidBody": {"kind": kind},
            "Collider": {"shape": collider},
        });
        self.ok("entity.spawn", json!({ "components": components }))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn y(&mut self, id: &str, world: &str) -> f64 {
        let t = self.ok(
            "component.get",
            json!({"entity": id, "component": "Transform", "world": world}),
        );
        t["value"]["translation"][1].as_f64().unwrap()
    }
}

fn ground() -> Value {
    json!({"Box": {"half_extents": [20.0, 0.5, 20.0]}})
}

fn cube() -> Value {
    json!({"Box": {"half_extents": [0.5, 0.5, 0.5]}})
}

#[test]
fn physics_components_are_reflected_and_steps_run_in_the_play_world() {
    let mut rig = Rig::new();
    rig.spawn(-0.5, "Static", ground());
    let id = rig.spawn(5.0, "Dynamic", cube());
    let step = rig.ok("sim.step", json!({"ticks": 180}));
    assert!(
        (rig.y(&id, "play") - 0.5).abs() < 0.05,
        "settled on the ground"
    );
    assert_eq!(rig.y(&id, "edit"), 5.0, "the scene is untouched");
    assert_eq!(step["world_hash"].as_str().unwrap().len(), 64);
}

#[test]
fn physics_raycast_works_on_edit_and_play_worlds() {
    let mut rig = Rig::new();
    let floor = rig.spawn(-0.5, "Static", ground());
    let id = rig.spawn(5.0, "Dynamic", cube());
    let down =
        json!({"origin": [0.0, 20.0, 0.0], "direction": [0.0, -1.0, 0.0], "max_distance": 100.0});
    let edit = rig.ok("physics.raycast", down.clone());
    assert_eq!(edit["hit"]["entity"], id.as_str(), "{edit}");
    assert!(
        (edit["hit"]["point"][1].as_f64().unwrap() - 5.5).abs() < 0.01,
        "{edit}"
    );
    rig.ok("sim.step", json!({"ticks": 180}));
    let mut play = down.clone();
    play["world"] = json!("play");
    let hit = rig.ok("physics.raycast", play);
    assert!(
        (hit["hit"]["point"][1].as_f64().unwrap() - 1.0).abs() < 0.05,
        "{hit}"
    );
    let mut miss = down;
    miss["layers"] = json!([5]);
    assert!(rig.ok("physics.raycast", miss)["hit"].is_null());
    let _ = floor;
}

#[test]
fn physics_raycast_validates_its_ray() {
    let mut rig = Rig::new();
    let err = rig
        .call(
            "physics.raycast",
            json!({"origin": [0.0, 0.0, 0.0], "direction": [0.0, 0.0, 0.0], "max_distance": 10.0}),
        )
        .unwrap_err();
    assert_eq!(err.code(), -32602, "{err:?}");
    let err = rig
        .call("physics.raycast", json!({"origin": [0.0, 0.0, 0.0], "direction": [0.0, -1.0, 0.0], "max_distance": 10.0, "layers": [16]}))
        .unwrap_err();
    assert_eq!(err.code(), -32602, "{err:?}");
    let method = rig.engine.methods().get("physics.raycast").unwrap();
    insta::assert_json_snapshot!(params_schema(method));
}

#[test]
fn collider_edits_apply_to_the_next_play_session_and_undo_restores_them() {
    let mut rig = Rig::new();
    rig.spawn(-0.5, "Static", ground());
    let id = rig.spawn(5.0, "Dynamic", cube());
    rig.ok("sim.step", json!({"ticks": 180}));
    assert!((rig.y(&id, "play") - 0.5).abs() < 0.05);
    let tall = json!({"shape": {"Box": {"half_extents": [0.5, 2.0, 0.5]}}});
    rig.ok(
        "component.set",
        json!({"entity": id, "component": "Collider", "value": tall}),
    );
    rig.ok("sim.stop", json!({}));
    rig.ok("sim.step", json!({"ticks": 180}));
    assert!(
        (rig.y(&id, "play") - 2.0).abs() < 0.05,
        "taller collider rests higher"
    );
    rig.ok("txn.undo", json!({}));
    rig.ok("sim.stop", json!({}));
    rig.ok("sim.step", json!({"ticks": 180}));
    assert!(
        (rig.y(&id, "play") - 0.5).abs() < 0.05,
        "undo restored the cube"
    );
}
