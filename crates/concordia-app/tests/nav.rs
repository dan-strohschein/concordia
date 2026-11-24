//! Navigation in a project (M2 phase 2.5a): `[nav]` adds the nav module, and a capture draws the
//! `navmesh` debug layer over a fixture level.

use concordia_api::{ApiErrorKind, Engine, EngineConfig};
use concordia_app::Project;
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";

struct Stage {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: concordia_core::id::SessionId,
}

impl Stage {
    fn new(manifest: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("concordia.toml"), manifest).unwrap();
        let project = Project::load(dir.path()).unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(9);
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut stage = Self {
            _dir: dir,
            engine,
            session,
        };
        stage.ok(
            "scene.new",
            json!({"path": "scenes/n.scene.ron", "name": "N", "actor": ACTOR}),
        );
        stage
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    fn spawn(&mut self, components: Value) {
        self.ok(
            "entity.spawn",
            json!({"components": components, "actor": ACTOR}),
        );
    }

    fn solid(&mut self, at: [f32; 3], half: [f32; 3], yaw: f32) {
        let (s, c) = (yaw / 2.0).sin_cos();
        self.spawn(json!({
            "Transform": {"translation": at, "rotation": [0.0, s, 0.0, c], "scale": half.map(|h| 2.0 * h)},
            "RigidBody": {"kind": "Static"},
            "Collider": {"shape": {"Box": {"half_extents": half}}},
            "MeshRenderer": {"mesh": "builtin:mesh/cube", "material": "builtin:material/default"},
        }));
    }
}

/// A 16 m floor with three blocks (one turned), and a grid wall that cuts off a corner room,
/// so the layer shows two islands in two colors.
fn fixture_level(stage: &mut Stage) {
    stage.spawn(json!({
        "Transform": {"translation": [0.0, -0.5, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [16.2, 1.0, 16.2]},
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [8.1, 0.5, 8.1]}}},
        "MeshRenderer": {"mesh": "builtin:mesh/cube", "material": "builtin:material/default"},
    }));
    stage.solid([-3.0, 1.0, -2.0], [1.0, 1.0, 1.0], 0.0);
    stage.solid([2.5, 1.0, 1.5], [1.5, 1.0, 0.5], 0.6);
    stage.solid([-1.0, 1.0, 4.0], [0.5, 1.0, 2.0], 0.0);
    let wall: Vec<String> = (0..16)
        .map(|z| {
            (0..16)
                .map(|x| {
                    if (x == 10 && z >= 10) || (z == 10 && x >= 10) {
                        '#'
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .collect();
    stage.spawn(json!({
        "Transform": {"translation": [-8.0, 0.0, -8.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        "NavAreaGrid": {"cell_size": 1.0, "rows": wall},
    }));
    stage.spawn(json!({
        "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        "NavMeshVolume": {"half_extents": [9.0, 3.0, 9.0]},
    }));
    let (eye, target) = ([0.0f32, 13.0, 10.0], [0.0f32, 0.0, 0.8]);
    stage.spawn(json!({
        "Transform": {"translation": eye, "rotation": quat_looking(eye, target), "scale": [1.0, 1.0, 1.0]},
        "Camera": {"exposure_ev100": 12.0},
    }));
    stage.spawn(json!({
        "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": quat_looking([0.0, 0.0, 0.0], [-0.4, -1.0, -0.6]), "scale": [1.0, 1.0, 1.0]},
        "DirectionalLight": {"color": [1.0, 1.0, 1.0], "illuminance_lux": 8000.0},
    }));
}

fn quat_looking(eye: [f32; 3], target: [f32; 3]) -> [f32; 4] {
    use concordia_core::math::{Mat3, Quat, Vec3};
    let forward = (Vec3::from_array(target) - Vec3::from_array(eye)).normalize();
    let right = forward.cross(Vec3::Y).normalize();
    Quat::from_mat3(&Mat3::from_cols(right, right.cross(forward), -forward)).to_array()
}

fn decode(shot: &Value) -> concordia_testkit::golden::RgbaImage {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(shot["image"]["data_base64"].as_str().unwrap())
        .unwrap();
    concordia_testkit::golden::RgbaImage::from_png(&bytes).unwrap()
}

fn has_gpu() -> bool {
    match concordia_rhi_metal::new_headless() {
        Err(concordia_rhi::RhiError::NoDevice { reason }) => {
            assert!(
                !concordia_testkit::gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            false
        }
        _ => true,
    }
}

#[test]
fn nav_section_adds_the_nav_module() {
    let mut stage = Stage::new("[physics]\n\n[nav]\n\n[assets]\n\n[render]\n");
    fixture_level(&mut stage);
    let built = stage.ok("nav.build", json!({}));
    assert_eq!(
        built["navmeshes"][0]["islands"], 2,
        "the wall cuts off the corner room: {built}"
    );
    assert!(stage.engine.debug_layers().contains(&"navmesh"));
    let without = Stage::new("[physics]\n");
    assert!(
        without.engine.methods().get("nav.build").is_none(),
        "no [nav], no nav.*"
    );
}

#[test]
fn nav_section_adds_the_crowd_module() {
    // Crowds are part of navigation: `[nav]` gives the `crowd` layer and steers a crowd agent (it
    // walks off its spot within a second). The fixture's floor is drawn, so it needs `[assets]`
    // and `[render]` too.
    let mut stage = Stage::new("[physics]\n\n[nav]\n\n[assets]\n\n[render]\n");
    assert!(stage.engine.debug_layers().contains(&"crowd"));
    fixture_level(&mut stage);
    let components = json!({
        "Transform": {"translation": [-6.0, 0.0, -6.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        "NavAgent": {"target": [-4.0, 0.0, -6.0]},
        "CrowdAgent": {},
    });
    let id = stage.ok(
        "entity.spawn",
        json!({"components": components, "actor": ACTOR}),
    )["id"]
        .clone();
    stage.ok("sim.step", json!({"ticks": 60}));
    let at = stage.ok(
        "component.get",
        json!({"entity": id, "component": "Transform", "world": "play"}),
    )["value"]["translation"][0]
        .as_f64()
        .unwrap();
    assert!(at > -5.5, "the crowd moved it: x = {at}");
    let without = Stage::new("[physics]\n");
    assert!(
        !without.engine.debug_layers().contains(&"crowd"),
        "no [nav], no crowd"
    );
}

#[test]
fn capture_rejects_unknown_debug_layers() {
    let mut stage = Stage::new("[physics]\n\n[nav]\n\n[assets]\n\n[render]\n");
    let e = stage
        .call(
            "capture.screenshot",
            json!({"width": 64, "height": 64, "debug": ["navmesh", "nope"]}),
        )
        .unwrap_err();
    assert_eq!(e.kind, ApiErrorKind::InvalidParams, "{e:?}");
    assert!(
        e.message.contains("nope") && e.message.contains("navmesh"),
        "names it and the known layers: {e:?}"
    );
}

#[test]
fn navmesh_debug_layer_draws_a_fixture_level() {
    if !has_gpu() {
        return;
    }
    let mut stage = Stage::new("[physics]\n\n[nav]\n\n[assets]\n\n[render]\n");
    fixture_level(&mut stage);
    let shot = |stage: &mut Stage, debug: Value| {
        decode(&stage.ok(
            "capture.screenshot",
            json!({"width": 400, "height": 300, "debug": debug}),
        ))
    };
    let plain = shot(&mut stage, json!([]));
    let drawn = shot(&mut stage, json!(["navmesh"]));
    let changed = plain
        .pixels()
        .chunks(4)
        .zip(drawn.pixels().chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        changed > 20_000,
        "the layer covers the floor ({changed} pixels)"
    );
    // The viewport's overlays name layers too.
    stage.ok("viewport.overlays", json!({"debug": ["navmesh"]}));
    let via_overlays = decode(&stage.ok(
        "capture.screenshot",
        json!({"width": 400, "height": 300, "overlays": true}),
    ));
    assert_eq!(via_overlays.pixels(), drawn.pixels());
    concordia_testkit::golden::assert_golden("nav", "navmesh_debug_layer", &drawn);
}

/// How many of two fresh agents on a floor plan on the first tick, under `manifest`.
fn planned_on_first_tick(manifest: &str) -> usize {
    let mut stage = Stage::new(manifest);
    stage.spawn(json!({
        "Transform": {"translation": [0.0, -0.5, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [8.1, 0.5, 8.1]}}},
    }));
    stage.spawn(json!({
        "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        "NavMeshVolume": {"half_extents": [10.0, 3.0, 10.0]},
    }));
    let mut ids = Vec::new();
    for z in [-2.0, 2.0] {
        let made = stage.ok(
            "entity.spawn",
            json!({"actor": ACTOR, "components": {
                "Transform": {"translation": [-4.0, 0.0, z], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
                "CharacterController": {},
                "CharacterMotion": {},
                "NavAgent": {"target": [4.0, 0.0, z]},
            }}),
        );
        ids.push(made["id"].as_str().unwrap().to_owned());
    }
    stage.ok("sim.step", json!({"ticks": 1}));
    ids.iter()
        .filter(|id| {
            let state = stage.ok(
                "component.get",
                json!({"entity": id, "component": "NavAgentState", "world": "play"}),
            );
            state["value"]["status"] == "Moving"
        })
        .count()
}

#[test]
fn nav_section_sets_plans_per_tick() {
    assert_eq!(
        planned_on_first_tick("[physics]\n\n[nav]\n"),
        2,
        "32 by default"
    );
    assert_eq!(
        planned_on_first_tick("[physics]\n\n[nav]\nplans_per_tick = 1\n"),
        1
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("concordia.toml"),
        "[nav]\nplans_per_tick = 0\n",
    )
    .unwrap();
    let e = Project::load(dir.path()).unwrap_err().to_string();
    assert!(e.contains("plans_per_tick"), "{e}");
}
