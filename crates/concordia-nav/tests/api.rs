//! The nav engine module: `nav.build` and `nav.path` over the Engine API, edits, the play world
//! and the `navmesh` debug layer.
//!
//! The fixture floor has half 10.1 (edges inside a cell) and the default volume (0.25 m cells,
//! radius 0.3 m = 2 cells, 64-cell tiles of 16 m): the ledge cell and two more are removed, so it
//! is walkable to ±9.5 at the surface y = −3 + 16 × 0.2 = 0.2. The 24 m volume is 2 × 2 tiles,
//! with seams at x = 4 and z = 4.

use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_api::{ApiErrorKind, Engine, EngineConfig, WorldSel};
use concordia_core::id::SessionId;
use concordia_nav::{NavMeshes, NavModule};
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
        config.deterministic_ids = Some(5);
        config.threads = NonZeroUsize::new(2).unwrap();
        config.journal.sync = false;
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

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    fn spawn(&mut self, components: Value) -> String {
        self.ok("entity.spawn", json!({ "components": components }))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn at(x: f32, y: f32, z: f32) -> Value {
        json!({"translation": [x, y, z], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]})
    }

    fn floor(&mut self) -> String {
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
}

fn near(v: &Value, want: [f64; 3], eps: f64) -> bool {
    (0..3).all(|i| (v[i].as_f64().unwrap() - want[i]).abs() <= eps)
}

#[test]
fn nav_build_over_api_reports_tiles_polygons_and_islands() {
    let mut rig = Rig::new();
    let e = rig.call("nav.build", json!({})).unwrap_err();
    assert_eq!(e.kind, ApiErrorKind::EntityNotFound, "{e:?}");
    assert!(e.message.contains("NavMeshVolume"), "{e:?}");
    assert!(e.data.suggestions[0].contains("NavMeshVolume"), "{e:?}");
    let volume = rig.floor();
    let first = rig.ok("nav.build", json!({}));
    let meshes = first["navmeshes"].as_array().unwrap();
    assert_eq!(meshes.len(), 1, "{first}");
    let m = &meshes[0];
    assert_eq!(m["volume"], json!(volume));
    assert_eq!(m["tiles"], 4);
    assert!(m["polygons"].as_u64().unwrap() >= 4, "{m}");
    assert_eq!(m["islands"], 1);
    assert_eq!(m["cached"], false);
    assert_eq!(m["key"].as_str().unwrap().len(), 64);
    assert!(near(&m["min"], [-9.5, 0.2, -9.5], 1e-3), "{m}");
    assert!(near(&m["max"], [9.5, 0.2, 9.5], 1e-3), "{m}");
    let second = rig.ok("nav.build", json!({"volume": volume}));
    assert_eq!(second["navmeshes"][0]["cached"], true, "{second}");
    assert_eq!(second["navmeshes"][0]["key"], m["key"]);
    let bad = rig.spawn(json!({
        "Transform": Rig::at(50.0, 0.0, 0.0),
        "NavMeshVolume": {"half_extents": [5.0, 0.0, 5.0]},
    }));
    let e = rig.call("nav.build", json!({"volume": bad})).unwrap_err();
    assert_eq!(e.kind, ApiErrorKind::ValueInvalid, "{e:?}");
    assert!(e.message.contains("half_extents"), "{e:?}");
}

#[test]
fn nav_path_over_api_returns_waypoints() {
    let mut rig = Rig::new();
    rig.floor();
    // Across the x = 4 seam on open floor: one segment, √(13² + 1²) long.
    let path = rig.ok(
        "nav.path",
        json!({"from": [-6.0, 0.0, 1.0], "to": [7.0, 0.0, 2.0]}),
    );
    assert_eq!(path["partial"], false, "{path}");
    let points = path["waypoints"].as_array().unwrap();
    assert_eq!(points.len(), 2, "{path}");
    assert!(
        near(&points[0], [-6.0, 0.2, 1.0], 1e-3) && near(&points[1], [7.0, 0.2, 2.0], 1e-3),
        "{path}"
    );
    assert!(
        (path["length"].as_f64().unwrap() - 170f64.sqrt()).abs() < 1e-3,
        "{path}"
    );
    // A wall across x ∈ [−0.5, 0.5], z ≤ 6 (a static box) is seen without a nav.build call.
    rig.spawn(json!({
        "Transform": Rig::at(0.0, 1.0, -2.0),
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [0.5, 1.0, 8.0]}}},
    }));
    let around = rig.ok(
        "nav.path",
        json!({"from": [-6.0, 0.0, 1.0], "to": [7.0, 0.0, 2.0]}),
    );
    assert!(
        around["waypoints"].as_array().unwrap().len() > 2,
        "{around}"
    );
    assert!(around["length"].as_f64().unwrap() > 13.5, "{around}");
    let off = rig
        .call(
            "nav.path",
            json!({"from": [80.0, 0.0, 0.0], "to": [0.0, 0.0, 0.0]}),
        )
        .unwrap_err();
    assert_eq!(off.kind, ApiErrorKind::ValueInvalid, "{off:?}");
    assert!(off.message.contains("navmesh"), "{off:?}");
    // The play world gets its own navmeshes, as a resource processors can read.
    rig.ok("sim.step", json!({"ticks": 1}));
    let play = rig.ok(
        "nav.path",
        json!({"from": [-6.0, 0.0, 1.0], "to": [7.0, 0.0, 2.0], "world": "play"}),
    );
    assert_eq!(play, around);
    let world = rig.engine.world(WorldSel::play).unwrap();
    let meshes = world
        .resource::<NavMeshes>()
        .expect("the play world has its navmeshes");
    assert_eq!(meshes.len(), 1);
}

#[test]
fn navmesh_debug_layer_draws_every_polygon() {
    let mut rig = Rig::new();
    assert!(rig.engine.debug_layers().contains(&"navmesh"));
    assert_eq!(
        rig.engine.debug_geometry("navmesh", WorldSel::edit),
        None,
        "nothing to draw yet"
    );
    rig.floor();
    let built = rig.ok("nav.build", json!({}));
    let polygons = built["navmeshes"][0]["polygons"].as_u64().unwrap() as usize;
    let geometry = rig
        .engine
        .debug_geometry("navmesh", WorldSel::edit)
        .expect("drawn");
    // Each polygon is a fan of (n − 2) triangles and an outline of n edges, lifted 2 cm.
    let sides: usize = geometry.lines.len();
    assert!(
        geometry.triangles.len() >= polygons && sides >= 3 * polygons,
        "{polygons} polygons"
    );
    assert_eq!(
        sides - geometry.triangles.len(),
        2 * polygons,
        "n edges and n − 2 triangles each"
    );
    for t in &geometry.triangles {
        assert!(t.points.iter().all(|p| (p.y - 0.22).abs() < 1e-3), "{t:?}");
    }
    assert_eq!(
        rig.engine.debug_geometry("no-such-layer", WorldSel::edit),
        None
    );
}
