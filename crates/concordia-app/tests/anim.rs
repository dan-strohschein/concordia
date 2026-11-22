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
    hero: String,
    moves: String,
    /// The instance's skinned entity.
    body: Value,
}

impl Stage {
    fn new() -> Self {
        Self::with("[assets]\n\n[anim]\n", Vec::new())
    }

    /// A stage with `manifest` and `extra` modules after the project's.
    fn with(manifest: &str, extra: Vec<std::sync::Arc<dyn concordia_api::EngineModule>>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("concordia.toml"), manifest).unwrap();
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
        config.modules.extend(extra);
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut stage = Self {
            _dir: dir,
            session,
            engine,
            hero: String::new(),
            moves: String::new(),
            body: Value::Null,
        };
        stage.ok(
            "scene.new",
            json!({"path": "scenes/a.scene.ron", "name": "A", "actor": ACTOR}),
        );
        stage.hero = stage.import("hero.fbx");
        stage.moves = stage.import("moves.fbx");
        let hero = stage.hero.clone();
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

/// A test-only module: when a body receives an `AnimEvent` named "step", its `Name` gains
/// " stepped" (gameplay receiving an animation event, as a real game's processor would).
#[derive(Debug)]
struct StepListener;

impl concordia_api::EngineModule for StepListener {
    fn name(&self) -> &'static str {
        "test-steps"
    }

    fn start_play(
        &self,
        _: &mut concordia_terra::World,
    ) -> Result<concordia_api::PlaySetup, concordia_api::ApiError> {
        use concordia_anim::AnimEvent;
        use concordia_scene::builtin::Name;
        use concordia_terra::{Phase, SystemCtx, View, processor};
        let mut setup = concordia_api::PlaySetup::default();
        setup.schedule.add_processor(
            processor("test.steps", Phase::PostPhysics)
                .receives::<AnimEvent>()
                .writes::<Name>()
                .run(|view: &mut View<'_>, _: &SystemCtx<'_>| {
                    let targets: Vec<_> = view
                        .signals::<AnimEvent>()
                        .unwrap()
                        .iter()
                        .filter(|(_, e)| e.name == "step")
                        .map(|(id, _)| *id)
                        .collect();
                    let names = view.query::<&mut Name>().unwrap();
                    names.for_each_of(&targets, |_, name| name.0.push_str(" stepped"));
                }),
        );
        Ok(setup)
    }
}

/// `id`'s world matrix in the play world, from its `Transform` and its parents'.
fn play_world_matrix(stage: &mut Stage, id: &Value) -> concordia_core::math::Mat4 {
    use concordia_core::math::{Mat4, Quat, Vec3};
    let mut m = Mat4::IDENTITY;
    let mut at = id.clone();
    loop {
        let t = stage.play_get(&at, "Transform");
        let v = |k: &str| -> Vec<f32> {
            t[k].as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap() as f32)
                .collect()
        };
        let (tr, r, s) = (v("translation"), v("rotation"), v("scale"));
        let local = Mat4::from_scale_rotation_translation(
            Vec3::from_slice(&s),
            Quat::from_slice(&r),
            Vec3::from_slice(&tr),
        );
        m = local * m;
        let got = stage.ok("entity.get", json!({"id": at, "world": "play"}));
        match got["components"].get("Parent") {
            Some(parent) => at = parent.clone(),
            None => return m,
        }
    }
}

#[test]
fn imported_character_raises_an_event_and_moves_an_attached_item() {
    let mut stage = Stage::with(
        "[assets]\n\n[anim]\n",
        vec![std::sync::Arc::new(StepListener)],
    );
    // Author a "step" at 0.25 s of Walk in the moves file's import settings, then re-import.
    let sidecar = stage._dir.path().join("moves.fbx.import.ron");
    let text = std::fs::read_to_string(&sidecar).unwrap();
    let events =
        r#"settings: {"clip_events": {"Armature|Walk": [{"time": 0.25, "name": "step"}]}}"#;
    std::fs::write(&sidecar, text.replace("settings: {}", events)).unwrap();
    assert_eq!(stage.import("moves.fbx"), stage.moves, "same asset");
    let body = stage.body.clone();
    let walk = format!("asset:{}#clip/Armature|Walk", stage.moves);
    stage.ok(
        "component.set",
        json!({"entity": body, "component": "AnimPlayer", "value": {"clip": walk}, "actor": ACTOR}),
    );
    // A sword attachment on the body (its bone set once the bones are known) holding a blade.
    let identity = json!({"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]});
    let sword = stage.ok(
        "entity.spawn",
        json!({"components": {"Transform": identity, "Parent": body}, "actor": ACTOR}),
    )["id"]
        .clone();
    let blade_local = json!({"translation": [0.0, 0.3, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]});
    let blade = stage.ok(
        "entity.spawn",
        json!({"components": {"Transform": blade_local, "Parent": sword}, "actor": ACTOR}),
    )["id"]
        .clone();
    let original_name = stage.play_name_or_edit(&body);

    stage.ok("sim.step", json!({"ticks": 1}));
    let seen = stage.ok("anim.inspect", json!({"entity": body}));
    assert_eq!(
        seen["clip"]["events"],
        json!([{"name": "step", "time": 0.25, "frame": 6.0}]),
        "the event, in frames at 24 fps"
    );
    let bones = seen["bones"].as_array().unwrap().clone();
    let (index, bone) = bones
        .iter()
        .enumerate()
        .rfind(|(_, b)| !b["parent"].is_null())
        .map(|(i, b)| (i, b["name"].as_str().unwrap().to_owned()))
        .unwrap();
    stage.ok(
        "component.set",
        json!({"entity": sword, "component": "BoneAttachment", "value": {"bone": bone}, "world": "play", "actor": ACTOR}),
    );
    let blade_before = play_world_matrix(&mut stage, &blade);

    // Step until the event is raised (pending), then once more until it is delivered.
    let mut raised_at = None;
    for tick in 2..=30 {
        stage.ok("sim.step", json!({"ticks": 1}));
        let pending = stage.ok("anim.inspect", json!({"entity": body}))["pending_events"].clone();
        if pending.as_array().is_some_and(|p| !p.is_empty()) {
            assert_eq!(pending[0]["name"], "step", "{pending}");
            assert_eq!(pending[0]["clip"], json!(walk));
            raised_at = Some(tick);
            break;
        }
    }
    let raised_at = raised_at.expect("the step is raised");
    assert!(
        (15..=16).contains(&raised_at),
        "0.25 s at 60 Hz: tick {raised_at}"
    );
    assert_eq!(
        stage.play_get(&body, "Name"),
        json!(original_name),
        "not delivered yet"
    );
    stage.ok("sim.step", json!({"ticks": 1}));
    assert_eq!(
        stage.play_get(&body, "Name"),
        json!(format!("{original_name} stepped")),
        "gameplay received it on the next tick"
    );

    // The sword sits on its bone: its world matrix is the bone's world transform.
    let seen = stage.ok("anim.inspect", json!({"entity": body}));
    let world = &seen["bones"][index]["world"];
    let at = |v: &Value| -> Vec<f32> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap() as f32)
            .collect()
    };
    let sword_world = play_world_matrix(&mut stage, &sword);
    let (_, _, sword_at) = sword_world.to_scale_rotation_translation();
    let bone_at = concordia_core::math::Vec3::from_slice(&at(&world["translation"]));
    assert!(
        (sword_at - bone_at).abs().max_element() < 1e-4,
        "{sword_at} vs {bone_at}"
    );
    let blade_after = play_world_matrix(&mut stage, &blade);
    assert_ne!(blade_before, blade_after, "the blade moved with the bone");
    let edit = stage.ok("entity.get", json!({"id": sword}));
    assert!(
        edit["components"].get("BoneAttachment").is_none(),
        "set in play only"
    );
}

impl Stage {
    /// `id`'s `Name` in the edit world (the play world copies it at the first tick).
    fn play_name_or_edit(&mut self, id: &Value) -> String {
        self.ok("component.get", json!({"entity": id, "component": "Name"}))["value"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

#[test]
fn skeleton_overlay_golden() {
    if let Err(concordia_rhi::RhiError::NoDevice { reason }) = concordia_rhi_metal::new_headless() {
        assert!(
            !concordia_testkit::gpu_required(),
            "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
        );
        return;
    }
    let mut stage = Stage::with("[assets]\n\n[anim]\n\n[render]\n", Vec::new());
    let body = stage.body.clone();
    let walk = format!("asset:{}#clip/Armature|Walk", stage.moves);
    stage.ok(
        "component.set",
        json!({"entity": body, "component": "AnimPlayer", "value": {"clip": walk}, "actor": ACTOR}),
    );
    let camera = json!({
        "Transform": {
            "translation": [2.6, 1.2, 0.8],
            "rotation": quat_looking([2.6, 1.2, 0.8], [0.0, 1.15, 0.0]),
            "scale": [1.0, 1.0, 1.0],
        },
        "Camera": {"exposure_ev100": 12.0},
    });
    stage.ok(
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
    stage.ok("entity.spawn", json!({"components": sun, "actor": ACTOR}));
    // Mid-stride (0.25 s), the skeleton drawn over the body and the body selected (posed box).
    stage.ok("sim.step", json!({"ticks": 15}));
    let shot = |stage: &mut Stage, overlays: bool| {
        decode(&stage.ok(
            "capture.screenshot",
            json!({"world": "play", "width": 320, "height": 240, "overlays": overlays}),
        ))
    };
    stage.ok(
        "viewport.overlays",
        json!({"skeleton": true, "selection": [body]}),
    );
    let plain = shot(&mut stage, false);
    let drawn = shot(&mut stage, true);
    let changed = plain
        .pixels()
        .chunks(4)
        .zip(drawn.pixels().chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        changed > 200,
        "the overlay draws bones and a box ({changed} pixels)"
    );
    concordia_testkit::golden::assert_golden(
        "anim",
        "skeleton_overlay_walk",
        &drawn,
        concordia_testkit::golden::Tolerance::mean(0.01),
    );
}

#[test]
fn inspected_bones_at_rest_sit_on_the_imported_bone_entities() {
    let mut stage = Stage::new();
    let body = stage.body.clone();
    // Move the whole instance so the entity's world matrix matters.
    let mut root = body.clone();
    loop {
        let got = stage.ok("entity.get", json!({"id": root}));
        match got["components"].get("Parent") {
            Some(parent) => root = parent.clone(),
            None => break,
        }
    }
    stage.ok(
        "component.patch",
        json!({"entity": root, "component": "Transform", "patch": {"translation": [3.0, 0.0, -2.0]}, "actor": ACTOR}),
    );
    stage.ok("sim.step", json!({"ticks": 1}));
    let seen = stage.ok("anim.inspect", json!({"entity": body}));
    // Every entity of the scene with a bone's name: its world position is the bone's.
    let named = stage.ok(
        "query.run",
        json!({"with": ["Name", "Transform"], "world": "play"}),
    );
    let ids: Vec<Value> = named["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].clone())
        .collect();
    let mut matched = 0;
    for bone in seen["bones"].as_array().unwrap() {
        let name = bone["name"].clone();
        for id in &ids {
            let entity = stage.ok("entity.get", json!({"id": id, "world": "play"}));
            if entity["components"]["Name"] != name {
                continue;
            }
            let at = play_world_matrix(&mut stage, id)
                .transform_point3(concordia_core::math::Vec3::ZERO);
            let want: Vec<f32> = bone["world"]["translation"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap() as f32)
                .collect();
            let want = concordia_core::math::Vec3::from_slice(&want);
            assert!(
                (at - want).abs().max_element() < 1e-3,
                "{name}: {at} vs {want}"
            );
            matched += 1;
        }
    }
    assert_eq!(matched, 7, "every bone has its entity");
}
