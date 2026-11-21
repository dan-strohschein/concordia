//! `[anim]`: an imported character plays a clip in the play world through the Engine API (M2 2.4.1).

use std::path::Path;

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";

#[test]
fn anim_module_plays_an_imported_clip_through_sim_step() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("concordia.toml"), "[assets]\n\n[anim]\n").unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fbx");
    for (from, to) in [
        ("skinned.fbx", "hero.fbx"),
        ("anim_stacks.fbx", "moves.fbx"),
    ] {
        std::fs::copy(fixtures.join(from), root.join(to)).unwrap();
    }
    let project = Project::load(root).unwrap();
    let mut config = EngineConfig::new(root);
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    let mut ok = |method: &str, params: Value| {
        engine
            .call(s, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    };
    ok(
        "scene.new",
        json!({"path": "scenes/a.scene.ron", "name": "A", "actor": ACTOR}),
    );
    let mut import = |path: &str| {
        ok("asset.import", json!({"path": path, "actor": ACTOR}))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let hero = import("hero.fbx");
    let moves = import("moves.fbx");
    let made = ok(
        "prefab.instantiate",
        json!({"asset": format!("asset:{hero}"), "actor": ACTOR}),
    );
    let body = made["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            ok("entity.get", json!({"id": e}))["components"]
                .get("Skinned")
                .is_some()
        })
        .expect("the instance has a skinned body")
        .clone();
    ok(
        "component.set",
        json!({
            "entity": body,
            "component": "AnimPlayer",
            "value": {"clip": format!("asset:{moves}#clip/Armature|Walk")},
            "actor": ACTOR,
        }),
    );
    ok("sim.step", json!({"ticks": 30}));
    let get = |ok: &mut dyn FnMut(&str, Value) -> Value, c: &str| {
        ok(
            "component.get",
            json!({"entity": body, "component": c, "world": "play"}),
        )["value"]
            .clone()
    };
    let player = get(&mut ok, "AnimPlayer");
    let t = player["time"].as_f64().unwrap();
    assert!((t - 0.5).abs() < 1e-4, "30 ticks at 60 Hz: {player}");
    let pose = get(&mut ok, "Pose");
    assert_eq!(pose["bones"].as_array().map(Vec::len), Some(7), "{pose}");
    ok("sim.step", json!({"ticks": 7}));
    assert_ne!(get(&mut ok, "Pose"), pose, "the pose follows the clip");
    let edit = ok("entity.get", json!({"id": body}));
    assert!(
        edit["components"].get("Pose").is_none(),
        "play never writes the edit world"
    );
}

/// Decodes a `capture.screenshot` result's PNG.
fn decode(shot: &Value) -> concordia_testkit::golden::RgbaImage {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(shot["image"]["data_base64"].as_str().unwrap())
        .unwrap();
    concordia_testkit::golden::RgbaImage::from_png(&bytes).unwrap()
}

#[test]
fn gpu_skinning_golden() {
    if let Err(concordia_rhi::RhiError::NoDevice { reason }) = concordia_rhi_metal::new_headless() {
        assert!(
            !concordia_testkit::gpu_required(),
            "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
        );
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("concordia.toml"),
        "[assets]\n\n[anim]\n\n[render]\n",
    )
    .unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fbx");
    for (from, to) in [
        ("skinned.fbx", "hero.fbx"),
        ("anim_stacks.fbx", "moves.fbx"),
    ] {
        std::fs::copy(fixtures.join(from), root.join(to)).unwrap();
    }
    let project = Project::load(root).unwrap();
    let mut config = EngineConfig::new(root);
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let s = engine.open_session();
    let mut ok = |method: &str, params: Value| {
        engine
            .call(s, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    };
    ok(
        "scene.new",
        json!({"path": "scenes/a.scene.ron", "name": "A", "actor": ACTOR}),
    );
    let mut import = |path: &str| {
        ok("asset.import", json!({"path": path, "actor": ACTOR}))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let hero = import("hero.fbx");
    let moves = import("moves.fbx");
    let made = ok(
        "prefab.instantiate",
        json!({"asset": format!("asset:{hero}"), "actor": ACTOR}),
    );
    let body = made["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            ok("entity.get", json!({"id": e}))["components"]
                .get("Skinned")
                .is_some()
        })
        .expect("the instance has a skinned body")
        .clone();
    ok(
        "component.set",
        json!({
            "entity": body,
            "component": "AnimPlayer",
            "value": {"clip": format!("asset:{moves}#clip/Armature|Walk")},
            "actor": ACTOR,
        }),
    );
    // A camera to the side (the legs swing front to back), and a sun from the front left.
    let camera = json!({
        "Transform": {
            "translation": [2.6, 1.2, 0.8],
            "rotation": quat_looking([2.6, 1.2, 0.8], [0.0, 1.15, 0.0]),
            "scale": [1.0, 1.0, 1.0],
        },
        "Camera": {"exposure_ev100": 12.0},
    });
    ok(
        "entity.spawn",
        json!({"components": camera, "actor": ACTOR}),
    );
    let sun = json!({
        "Transform": {
            "translation": [0.0, 0.0, 0.0],
            "rotation": quat_looking([0.0, 0.0, 0.0], [-0.5, -1.0, -0.7]),
            "scale": [1.0, 1.0, 1.0],
        },
        "DirectionalLight": {"color": [1.0, 1.0, 1.0], "illuminance_lux": 8000.0},
    });
    ok("entity.spawn", json!({"components": sun, "actor": ACTOR}));
    let shot = |ok: &mut dyn FnMut(&str, Value) -> Value, world: &str| {
        ok(
            "capture.screenshot",
            json!({"world": world, "width": 320, "height": 240}),
        )
    };
    let bind = decode(&shot(&mut ok, "edit"));
    // Mid-stride: the fixture's Walk swings by sin(t·τ), so 15 ticks at 60 Hz (0.25 s of the 1 s
    // clip) is its widest step; at 0.5 s every rotation is back at zero.
    ok("sim.step", json!({"ticks": 15}));
    let walking = shot(&mut ok, "play");
    assert_eq!(walking["sim_tick"], 15);
    let walking = decode(&walking);
    let moved = bind
        .pixels()
        .chunks(4)
        .zip(walking.pixels().chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        moved > 500,
        "the play world draws the pose, not the bind mesh ({moved} pixels)"
    );
    concordia_testkit::golden::assert_golden(
        "anim",
        "gpu_skinning_walk",
        &walking,
        concordia_testkit::golden::Tolerance::mean(0.01),
    );
}

/// A rotation whose -Z looks from `eye` toward `target` (Y up), as `[x, y, z, w]`.
fn quat_looking(eye: [f32; 3], target: [f32; 3]) -> [f32; 4] {
    use concordia_core::math::{Mat3, Quat, Vec3};
    let forward = (Vec3::from_array(target) - Vec3::from_array(eye)).normalize();
    let right = forward.cross(Vec3::Y).normalize();
    let rotation = Quat::from_mat3(&Mat3::from_cols(right, right.cross(forward), -forward));
    rotation.to_array()
}

/// An engine with `[assets]` and `[anim]`, the FBX character as `hero.fbx` and its actions as
/// `moves.fbx`, both imported, and one instance of the character.
struct Stage {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: concordia_core::id::SessionId,
    moves: String,
    /// The instance's skinned entity.
    body: Value,
}

impl Stage {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("concordia.toml"), "[assets]\n\n[anim]\n").unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fbx");
        for (from, to) in [
            ("skinned.fbx", "hero.fbx"),
            ("anim_stacks.fbx", "moves.fbx"),
        ] {
            std::fs::copy(fixtures.join(from), root.join(to)).unwrap();
        }
        let project = Project::load(root).unwrap();
        let mut config = EngineConfig::new(root);
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut stage = Self {
            _dir: dir,
            session,
            engine,
            moves: String::new(),
            body: Value::Null,
        };
        stage.ok(
            "scene.new",
            json!({"path": "scenes/a.scene.ron", "name": "A", "actor": ACTOR}),
        );
        let hero = stage.import("hero.fbx");
        stage.moves = stage.import("moves.fbx");
        stage.body = stage.instance(&hero);
        stage
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    fn import(&mut self, path: &str) -> String {
        self.ok("asset.import", json!({"path": path, "actor": ACTOR}))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// Instantiates `asset:<model>` and returns its skinned entity.
    fn instance(&mut self, model: &str) -> Value {
        let made = self.ok(
            "prefab.instantiate",
            json!({"asset": format!("asset:{model}"), "actor": ACTOR}),
        );
        made["entities"]
            .as_array()
            .unwrap()
            .clone()
            .into_iter()
            .find(|e| {
                self.ok("entity.get", json!({"id": e}))["components"]
                    .get("Skinned")
                    .is_some()
            })
            .expect("the instance has a skinned body")
    }

    /// Writes and imports `anim/<name>.animgraph.ron`; `{walk}` and `{idle}` name the clips.
    fn graph(&mut self, name: &str, text: &str) -> String {
        let clip = |n: &str| format!("asset:{}#clip/Armature|{n}", self.moves);
        let text = text
            .replace("{walk}", &clip("Walk"))
            .replace("{idle}", &clip("Idle"));
        let path = format!("anim/{name}.animgraph.ron");
        let root = self._dir.path();
        std::fs::create_dir_all(root.join("anim")).unwrap();
        std::fs::write(root.join(&path), text).unwrap();
        format!("asset:{}", self.import(&path))
    }

    fn play_get(&mut self, entity: &Value, component: &str) -> Value {
        self.ok(
            "component.get",
            json!({"entity": entity, "component": component, "world": "play"}),
        )["value"]
            .clone()
    }
}

const MOVES: &str = r#"(
    parameters: {"moving": Bool(false)},
    nodes: {
        "idle": Clip(clip: "{idle}"),
        "walk": Clip(clip: "{walk}"),
        "moves": StateMachine(
            states: {"idle": "idle", "walk": "walk"},
            start: "walk",
            transitions: [
                (from: "walk", to: "idle", xfade: 0.25),
                (from: "idle", to: "walk", advance: Auto, condition: Some("moving")),
            ],
        ),
    },
    root: "moves",
)"#;

#[test]
fn anim_graph_plays_an_imported_character_through_sim_step() {
    let mut stage = Stage::new();
    let graph = stage.graph("moves", MOVES);
    let body = stage.body.clone();
    stage.ok(
        "component.set",
        json!({"entity": body, "component": "AnimGraphPlayer", "value": {"graph": graph}, "actor": ACTOR}),
    );
    // A twin playing Walk as a clip: the graph's walk state must pose it identically.
    let hero = stage.ok("entity.get", json!({"id": body}))["components"]["Skinned"]["skin"]
        .as_str()
        .unwrap()
        .split('#')
        .next()
        .unwrap()
        .trim_start_matches("asset:")
        .to_owned();
    let twin = stage.instance(&hero);
    let walk = format!("asset:{}#clip/Armature|Walk", stage.moves);
    stage.ok(
        "component.set",
        json!({"entity": twin, "component": "AnimPlayer", "value": {"clip": walk}, "actor": ACTOR}),
    );
    stage.ok("sim.step", json!({"ticks": 30}));
    let pose = stage.play_get(&body, "Pose");
    assert_eq!(pose["bones"].as_array().map(Vec::len), Some(7), "{pose}");
    assert_eq!(
        pose,
        stage.play_get(&twin, "Pose"),
        "the walk state is the Walk clip"
    );
    let state = stage.play_get(&body, "AnimGraphState");
    assert_eq!(state["nodes"].as_array().map(Vec::len), Some(3), "{state}");

    let edit = stage.ok("entity.get", json!({"id": body}));
    assert!(
        edit["components"].get("AnimGraphState").is_none()
            && edit["components"].get("Pose").is_none(),
        "play never writes the edit world"
    );
}

#[test]
fn graph_parameters_set_by_component_patch_take_effect_next_tick() {
    let mut stage = Stage::new();
    let graph = stage.graph("moves", MOVES);
    let body = stage.body.clone();
    stage.ok(
        "component.set",
        json!({"entity": body, "component": "AnimGraphPlayer", "value": {"graph": graph}, "actor": ACTOR}),
    );
    // Node order is by name (idle, moves, walk); the machine's states too (idle 0, walk 1).
    let current =
        |stage: &mut Stage| stage.play_get(&body, "AnimGraphState")["nodes"][1]["current"].clone();
    stage.ok("sim.step", json!({"ticks": 5}));
    assert_eq!(current(&mut stage), 1, "starts in walk");

    stage.ok(
        "component.patch",
        json!({"entity": body, "component": "AnimGraphPlayer", "patch": {"travel": {"moves": "idle"}}, "world": "play", "actor": ACTOR}),
    );
    let pose = stage.play_get(&body, "Pose");
    assert_eq!(current(&mut stage), 1, "nothing changes until a tick runs");
    stage.ok("sim.step", json!({"ticks": 1}));
    assert_eq!(current(&mut stage), 0, "the next tick travels to idle");
    assert_ne!(stage.play_get(&body, "Pose"), pose);
    let player = stage.play_get(&body, "AnimGraphPlayer");
    assert_eq!(
        player["travel"],
        json!({}),
        "the request was consumed: {player}"
    );

    stage.ok("sim.step", json!({"ticks": 20}));
    stage.ok(
        "component.patch",
        json!({"entity": body, "component": "AnimGraphPlayer", "patch": {"bools": {"moving": true}}, "world": "play", "actor": ACTOR}),
    );
    assert_eq!(current(&mut stage), 0);
    stage.ok("sim.step", json!({"ticks": 1}));
    assert_eq!(
        current(&mut stage),
        1,
        "moving takes the auto transition next tick"
    );
    let edit = stage.ok(
        "component.get",
        json!({"entity": body, "component": "AnimGraphPlayer"}),
    );
    assert_eq!(
        edit["value"]["bools"],
        json!({}),
        "the edit world keeps its value"
    );
}
