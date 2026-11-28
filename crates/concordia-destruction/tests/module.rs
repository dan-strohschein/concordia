//! `[destruction]` through the Engine API (M8 8.3): a generated slab, fractured by
//! `fracture.generate` with weak bonds, falls in play and breaks on the ground. The edit world's
//! slab is never touched (ADR-0032).

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use concordia_core::id::SessionId;
use serde_json::{Value, json};

const ACTOR: &str = "agent:tech-artist";
const RECIPE: &str = "breakables/slab.breakable.ron";

struct Rig {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("concordia.toml"),
            "[physics]\n\n[assets]\n\n[gen]\n\n[fracture]\n\n[destruction]\n",
        )
        .unwrap();
        let project = Project::load(dir.path()).unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        Self {
            _dir: dir,
            engine,
            session,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    /// A scene with ground and a slab breakable 3 m up; returns the slab's id.
    fn scene(&mut self) -> Value {
        let out = self.ok(
            "gen.run",
            json!({"provider": "mesh", "request": {"Box": {"size": [2.0, 1.0, 1.0]}},
                   "path": "assets/models/slab.glb", "actor": ACTOR}),
        );
        let mesh = out["sub_assets"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|s| s.starts_with("mesh/"))
            .unwrap()
            .to_owned();
        let mesh = format!("asset:{}#{mesh}", out["id"].as_str().unwrap());
        self.ok(
            "scene.new",
            json!({"path": "scenes/test.scene.ron", "name": "Test", "actor": ACTOR}),
        );
        self.ok(
            "fracture.generate",
            json!({"mesh": mesh, "pattern": {"Slices": {"normal": [1.0, 0.0, 0.0], "count": 2}},
                   "path": RECIPE, "actor": ACTOR,
                   "structure": {"density": 1000.0, "strength": 0.001, "levels": [], "edges": []}}),
        );
        let at = |y: f32| json!({"translation": [0.0, y, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]});
        self.ok(
            "entity.spawn",
            json!({"components": {
                "Transform": at(-0.5),
                "RigidBody": {"kind": "Static"},
                "Collider": {"shape": {"Box": {"half_extents": [30.0, 0.5, 30.0]}}}
            }, "actor": ACTOR}),
        );
        self.ok(
            "entity.spawn",
            json!({"components": {
                "Transform": at(3.0),
                "RigidBody": {"kind": "Dynamic"},
                "Breakable": {"recipe": RECIPE, "anchors": [], "impact_radius": 5.0}
            }, "actor": ACTOR}),
        )["id"]
            .clone()
    }

    fn component(&mut self, id: &Value, name: &str, world: &str) -> Option<Value> {
        self.call(
            "component.get",
            json!({"entity": id, "component": name, "world": world}),
        )
        .ok()
    }
}

#[test]
fn a_breakable_breaks_in_play_and_edit_world_is_never_broken() {
    let mut rig = Rig::new();
    let slab = rig.scene();
    rig.ok("sim.step", json!({"ticks": 10}));
    assert!(
        rig.component(&slab, "Fragment", "play").is_some(),
        "in play the slab became one fragment body"
    );
    rig.ok("sim.step", json!({"ticks": 110}));
    assert!(
        rig.component(&slab, "Fragment", "play").is_none(),
        "it hit the ground and split into its two halves"
    );
    // The edit world's slab is as authored: still a Breakable, never a fragment or a body.
    assert!(rig.component(&slab, "Breakable", "edit").is_some());
    assert!(rig.component(&slab, "Fragment", "edit").is_none());
    assert!(rig.component(&slab, "Collider", "edit").is_none());
}
