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
        Self::with(Vec::new())
    }

    /// The physics module and `extra` modules.
    fn with(extra: Vec<Arc<dyn concordia_api::EngineModule>>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(3);
        config.threads = NonZeroUsize::MIN;
        config.journal.sync = false;
        config.modules = vec![Arc::new(PhysicsModule::new(PhysicsSettings::default()))];
        config.modules.extend(extra);
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

type Hits = Arc<std::sync::Mutex<Vec<(u64, Option<concordia_physics::RayHit>)>>>;

/// A test module whose processor casts a ray straight down from (0, 10, 0) every tick.
#[derive(Debug)]
struct Prober(Hits);

impl concordia_api::EngineModule for Prober {
    fn name(&self) -> &'static str {
        "test-prober"
    }

    fn start_play(
        &self,
        _: &mut concordia_terra::World,
    ) -> Result<concordia_api::PlaySetup, concordia_api::ApiError> {
        use concordia_core::math::{Ray, Vec3};
        use concordia_physics::{LayerMask, PhysicsQueries};
        use concordia_terra::{Phase, SystemCtx, View, processor};
        let hits = self.0.clone();
        let mut setup = concordia_api::PlaySetup::default();
        setup.schedule.add_processor(
            processor("test.probe", Phase::PrePhysics)
                .reads_resource::<PhysicsQueries>()
                .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                    let ray = Ray::new(Vec3::new(0.0, 10.0, 0.0), Vec3::NEG_Y).unwrap();
                    let hit = view
                        .resource::<PhysicsQueries>()
                        .ok()
                        .and_then(|q| q.raycast(&ray, 100.0, LayerMask::ALL));
                    hits.lock().unwrap().push((ctx.tick.0, hit));
                }),
        );
        Ok(setup)
    }
}

#[test]
fn processors_raycast_through_physics_queries() {
    // Queries see the bodies as of the last physics step: nothing on tick 0, then the ground's
    // top (y = 0.5 for a box of half 0.5 at y = 0) 9.5 m below the ray's start.
    let hits: Hits = Arc::default();
    let mut rig = Rig::with(vec![Arc::new(Prober(hits.clone()))]);
    let ground_id = rig.spawn(0.0, "Static", ground());
    rig.ok("sim.step", json!({"ticks": 2}));
    let got = hits.lock().unwrap().clone();
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[0], (0, None), "nothing is stepped before tick 0");
    let (tick, hit) = got[1];
    assert_eq!(tick, 1);
    let hit = hit.expect("the ground");
    assert_eq!(hit.entity.to_string(), ground_id);
    assert!((hit.distance - 9.5).abs() < 1e-4, "{hit:?}");
    let api = rig.ok(
        "physics.raycast",
        json!({"origin": [0.0, 10.0, 0.0], "direction": [0.0, -1.0, 0.0], "max_distance": 100.0, "world": "play"}),
    );
    assert_eq!(api["hit"]["entity"], ground_id.as_str());
    assert!(
        (api["hit"]["distance"].as_f64().unwrap() - f64::from(hit.distance)).abs() < 1e-6,
        "the same hit as physics.raycast: {api}"
    );
}

#[test]
fn contact_signals_are_registered_for_scripts() {
    // A gameplay module names engine signals by their registered names (ADR-0062), as it does
    // `NavArrived`; physics must register `ContactBegan` and `ContactEnded` (found in M2 phase
    // 2.10c.1: a script receiving them failed with "names unknown type \"ContactBegan\"").
    let mut rig = Rig::new();
    for name in ["ContactBegan", "ContactEnded"] {
        let schema = rig.ok("schema.get", json!({"name": name}));
        let fields = &schema["schema"]["properties"];
        assert!(
            fields["other"].is_object() && fields["sensor"].is_object(),
            "{schema}"
        );
    }
}
