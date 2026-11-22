//! Windowed play (M2 phase 2.5b.2b, ADR-0078): `Player` turns window events into Engine API
//! calls, driven here by synthetic events on a headless engine.
//!
//! **The camera** looks from (0, 14, 12) at the origin (perspective, fov_y 0.8, near 0.1). The
//! centre of the window is its forward ray: (0, −14, −12) / √340 = (0, −0.759257, −0.650791),
//! from 0.1 m along it in depth. The near plane is 0.1 m along the camera's −Z axis, which is
//! that same forward direction, so the start is (0, 14, 12) + 0.1 × forward =
//! (0, 13.924074, 11.934921). A 1920 × 1080 window at scale 2 is 960 × 540 logical pixels, so
//! its centre is the cursor (480, 270).
//!
//! **Click-to-move**: the marker (4, 0, −3) projected into the 960 × 540 image is clicked; the
//! ray hits the floor there and snaps to (4, 0.2, −3), as in `click_to_move.rs`.

use concordia_api::{ApiError, Engine, EngineConfig, WorldSel};
use concordia_app::{Player, Project};
use concordia_core::math::Vec3;
use concordia_input::{ActionStates, ActionValue, InputEvent, KeyCode, MouseButton};
use concordia_platform::WindowEvent;
use concordia_render::{Projection, View};
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";
const MAP: &str = r#"InputMap(
    actions: {
        "jump": (kind: Button, bindings: [Key(Space)]),
        "move": (kind: Button, bindings: [MouseButton(Left)]),
        "pointer_ray": (kind: Ray, bindings: [PointerRay]),
    },
)"#;
const MANIFEST: &str =
    "[input]\nmap = \"input/game.input.ron\"\n\n[render]\n\n[physics]\n\n[nav]\n";

struct Stage {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: concordia_core::id::SessionId,
}

impl Stage {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("concordia.toml"), MANIFEST).unwrap();
        std::fs::create_dir_all(dir.path().join("input")).unwrap();
        std::fs::write(dir.path().join("input/game.input.ron"), MAP).unwrap();
        let project = Project::load(dir.path()).unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(17);
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
            json!({"path": "scenes/p.scene.ron", "name": "P", "actor": ACTOR}),
        );
        stage
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    fn spawn(&mut self, components: Value) -> String {
        self.ok(
            "entity.spawn",
            json!({"components": components, "actor": ACTOR}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// Sends `event` to `player` with this engine as its API.
    fn feed(&mut self, player: &mut Player, event: WindowEvent) {
        let (engine, session) = (&mut self.engine, self.session);
        let mut api = |method: &str, params: Value| engine.call(session, method, params);
        player.event(&event, &mut api).unwrap();
    }

    fn step(&mut self) {
        self.ok("sim.step", json!({"ticks": 1}));
    }

    fn action(&self, name: &str) -> concordia_input::ActionState {
        let world = self.engine.world(WorldSel::play).unwrap();
        *world.resource::<ActionStates>().unwrap().get(name).unwrap()
    }

    fn camera(&mut self) {
        let view = camera_view();
        let (_, rotation, eye) = view.world_matrix().unwrap().to_scale_rotation_translation();
        self.spawn(json!({
            "Transform": at(eye.to_array(), rotation.to_array()),
            "Camera": {"projection": {"Perspective": {"fov_y": 0.8, "near": 0.1, "far": 100.0}}},
        }));
    }
}

fn at(p: [f32; 3], rotation: [f32; 4]) -> Value {
    json!({"translation": p, "rotation": rotation, "scale": [1.0, 1.0, 1.0]})
}

fn camera_view() -> View {
    View {
        eye: Vec3::new(0.0, 14.0, 12.0),
        target: Vec3::ZERO,
        projection: Projection::Perspective {
            fov_y: 0.8,
            near: 0.1,
            far: 100.0,
        },
    }
}

fn opened(width: u32, height: u32, scale_factor: f64) -> WindowEvent {
    WindowEvent::Opened {
        width,
        height,
        scale_factor,
        view: None,
    }
}

fn input(event: InputEvent) -> WindowEvent {
    WindowEvent::Input(event)
}

#[test]
fn player_starts_play_and_presents_the_game_camera() {
    let mut calls: Vec<(String, Value)> = Vec::new();
    let mut api = |method: &str, params: Value| -> Result<Value, ApiError> {
        calls.push((method.to_owned(), params));
        Ok(json!({}))
    };
    let mut player = Player::new(true);
    for event in [
        opened(1280, 720, 2.0),
        WindowEvent::Resized {
            width: 800,
            height: 600,
        },
        WindowEvent::CloseRequested,
    ] {
        player.event(&event, &mut api).unwrap();
    }
    let present = |w: u32, h: u32| json!({"width": w, "height": h, "world": "play"});
    assert_eq!(
        calls,
        [
            ("sim.play".to_owned(), json!({})),
            ("viewport.present".to_owned(), present(1280, 720)),
            ("viewport.present".to_owned(), present(800, 600)),
            ("sim.stop".to_owned(), json!({})),
        ]
    );
}

#[test]
fn window_input_reaches_the_play_world() {
    let mut stage = Stage::new();
    let mut player = Player::new(false);
    stage.feed(&mut player, opened(1280, 720, 1.0));
    stage.feed(
        &mut player,
        input(InputEvent::Key {
            key: KeyCode::Space,
            pressed: true,
        }),
    );
    stage.step();
    let jump = stage.action("jump");
    assert!(jump.pressed && jump.just_pressed, "{jump:?}");
    stage.feed(
        &mut player,
        input(InputEvent::Key {
            key: KeyCode::Space,
            pressed: false,
        }),
    );
    stage.step();
    assert!(stage.action("jump").just_released);
}

#[test]
fn cursor_moves_become_rays_from_the_drawing_camera() {
    let mut stage = Stage::new();
    stage.camera();
    let mut player = Player::new(false);
    stage.feed(&mut player, opened(1920, 1080, 2.0));
    stage.step();
    stage.feed(
        &mut player,
        input(InputEvent::PointerMove { x: 480.0, y: 270.0 }),
    );
    stage.step();
    let ActionValue::Ray { origin, direction } = stage.action("pointer_ray").value else {
        panic!("a ray")
    };
    let close = |a: Vec3, b: Vec3| (a - b).abs().max_element() < 1e-4;
    assert!(
        close(direction, Vec3::new(0.0, -0.759_257, -0.650_791)),
        "{direction}"
    );
    assert!(
        close(origin, Vec3::new(0.0, 13.924_074, 11.934_921)),
        "{origin}"
    );
}

#[test]
fn focus_loss_releases_held_input() {
    let mut stage = Stage::new();
    let mut player = Player::new(false);
    stage.feed(&mut player, opened(1280, 720, 1.0));
    stage.feed(
        &mut player,
        input(InputEvent::Key {
            key: KeyCode::Space,
            pressed: true,
        }),
    );
    stage.feed(
        &mut player,
        input(InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        }),
    );
    stage.step();
    assert!(stage.action("jump").pressed && stage.action("move").pressed);
    stage.feed(&mut player, WindowEvent::Focused(false));
    stage.step();
    assert!(!stage.action("jump").pressed, "{:?}", stage.action("jump"));
    assert!(!stage.action("move").pressed, "{:?}", stage.action("move"));
    // Nothing is held now, so losing focus again sends nothing.
    stage.feed(&mut player, WindowEvent::Focused(false));
}

#[test]
fn clicking_in_the_window_moves_the_hero() {
    let mut stage = Stage::new();
    stage.spawn(json!({
        "Transform": at([0.0, -0.5, 0.0], [0.0, 0.0, 0.0, 1.0]),
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [10.1, 0.5, 10.1]}}},
    }));
    stage.spawn(json!({
        "Transform": at([0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
        "NavMeshVolume": {"half_extents": [12.0, 3.0, 12.0]},
    }));
    stage.camera();
    let hero = stage.spawn(json!({
        "Transform": at([-5.0, 0.0, 4.0], [0.0, 0.0, 0.0, 1.0]),
        "CharacterController": {},
        "CharacterMotion": {},
        "NavAgent": {"max_speed": 4.0},
        "ClickToMove": {},
    }));
    let mut player = Player::new(false);
    stage.feed(&mut player, opened(960, 540, 1.0));
    stage.step();
    let [x, y] = camera_view()
        .project(Vec3::new(4.0, 0.0, -3.0), 960.0, 540.0)
        .unwrap();
    stage.feed(&mut player, input(InputEvent::PointerMove { x, y }));
    stage.feed(
        &mut player,
        input(InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        }),
    );
    stage.step();
    let target = stage.ok(
        "component.get",
        json!({"entity": hero, "component": "NavAgent", "world": "play"}),
    )["value"]["target"]
        .clone();
    let t: Vec<f64> = (0..3).map(|i| target[i].as_f64().unwrap()).collect();
    assert!(
        (t[0] - 4.0).abs() < 1e-3 && (t[1] - 0.2).abs() < 1e-3 && (t[2] + 3.0).abs() < 1e-3,
        "{target}"
    );
}

#[test]
fn a_window_without_a_camera_still_plays() {
    // No camera: the ray is skipped (warned once), and keys still reach the play world.
    let mut stage = Stage::new();
    let mut player = Player::new(false);
    stage.feed(&mut player, opened(1280, 720, 1.0));
    stage.step();
    stage.feed(
        &mut player,
        input(InputEvent::PointerMove { x: 10.0, y: 10.0 }),
    );
    stage.feed(
        &mut player,
        input(InputEvent::Key {
            key: KeyCode::Space,
            pressed: true,
        }),
    );
    stage.step();
    assert!(stage.action("jump").pressed);
    assert!(!stage.action("pointer_ray").pressed, "no camera, no ray");
}
