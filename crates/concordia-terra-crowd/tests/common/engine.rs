//! An Engine with physics, nav and the crowd over the Engine API.
#![allow(dead_code)]

use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_api::{Engine, EngineConfig, EngineModule};
use concordia_core::id::SessionId;
use concordia_nav::NavModule;
use concordia_physics::{PhysicsModule, PhysicsSettings};
use concordia_terra_crowd::CrowdModule;
use serde_json::{Value, json};

pub struct Engined {
    pub dir: tempfile::TempDir,
    pub engine: Engine,
    session: SessionId,
}

impl Engined {
    /// `first` modules, then physics, nav and the crowd, with `threads` job threads; `setup`
    /// writes project files before the engine opens.
    pub fn new(
        threads: usize,
        first: Vec<Arc<dyn EngineModule>>,
        setup: impl FnOnce(&std::path::Path),
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        setup(dir.path());
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(5);
        config.threads = NonZeroUsize::new(threads).unwrap();
        config.journal.sync = false;
        config.modules = first;
        config
            .modules
            .push(Arc::new(PhysicsModule::new(PhysicsSettings::default())));
        config.modules.push(Arc::new(NavModule::new()));
        config.modules.push(Arc::new(CrowdModule::new()));
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut rig = Self {
            dir,
            engine,
            session,
        };
        rig.ok(
            "session.open",
            json!({"client": "test", "actor": "agent:test"}),
        );
        rig.ok(
            "scene.new",
            json!({"path": "scenes/c.scene.ron", "name": "C"}),
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

    /// A floor of half 10.1 (walkable to ±9.5 at y = 0.2 with the default 0.3 m volume) and its
    /// 24 m volume.
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

/// `v` (a JSON [x, y, z]) is within `eps` of `want` on every axis.
pub fn near(v: &Value, want: [f32; 3], eps: f32) -> bool {
    (0..3).all(|i| (v[i].as_f64().unwrap() as f32 - want[i]).abs() <= eps)
}
