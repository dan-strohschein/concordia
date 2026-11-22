//! An Engine with physics and nav over the Engine API, for the API and agent tests.

use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_api::{Engine, EngineConfig};
use concordia_core::id::SessionId;
use concordia_core::log::LogRing;
use concordia_nav::NavModule;
use concordia_physics::{PhysicsModule, PhysicsSettings};
use serde_json::{Value, json};

pub struct Rig {
    _dir: tempfile::TempDir,
    pub engine: Engine,
    session: SessionId,
}

impl Rig {
    pub fn new() -> Self {
        Self::with(2, None)
    }

    /// An engine with `threads` job threads, logging into `ring` if given.
    pub fn with(threads: usize, ring: Option<Arc<LogRing>>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(5);
        config.threads = NonZeroUsize::new(threads).unwrap();
        config.journal.sync = false;
        config.log_ring = ring;
        config.modules = vec![
            Arc::new(PhysicsModule::new(PhysicsSettings::default())),
            Arc::new(NavModule::new()),
        ];
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
            json!({"path": "scenes/n.scene.ron", "name": "N"}),
        );
        rig
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    pub fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    pub fn spawn(&mut self, components: Value) -> String {
        self.ok("entity.spawn", json!({ "components": components }))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub fn at(x: f32, y: f32, z: f32) -> Value {
        json!({"translation": [x, y, z], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]})
    }

    /// A floor of half 10.1 (walkable to ±9.5 at y = 0.2 with the default volume) and its 24 m
    /// volume; returns the volume.
    pub fn floor(&mut self) -> String {
        self.spawn(json!({
            "Transform": Self::at(0.0, -0.5, 0.0),
            "RigidBody": {"kind": "Static"},
            "Collider": {"shape": {"Box": {"half_extents": [10.1, 0.5, 10.1]}}},
        }));
        self.spawn(json!({
            "Transform": Self::at(0.0, 0.0, 0.0),
            "NavMeshVolume": {"half_extents": [12.0, 3.0, 12.0]},
        }))
    }

    /// A walking character at (x, 0, z) with a default controller and `agent`.
    pub fn character(&mut self, x: f32, z: f32, agent: Value) -> String {
        self.spawn(json!({
            "Transform": Self::at(x, 0.0, z),
            "CharacterController": {},
            "CharacterMotion": {},
            "NavAgent": agent,
        }))
    }

    /// A play-world component value.
    pub fn play(&mut self, entity: &str, component: &str) -> Value {
        self.ok(
            "component.get",
            json!({"entity": entity, "component": component, "world": "play"}),
        )["value"]
            .clone()
    }

    /// Steps and returns the play world's hash.
    pub fn step(&mut self, ticks: u32) -> String {
        self.ok("sim.step", json!({"ticks": ticks}))["world_hash"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

pub fn near(v: &Value, want: [f64; 3], eps: f64) -> bool {
    (0..3).all(|i| (v[i].as_f64().unwrap() - want[i]).abs() <= eps)
}
