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
use concordia_platform::{CursorIcon, WindowEvent};
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

    /// Opens the window (`Player` starts play), then pauses: a playing engine also advances by
    /// wall-clock time on every call, so only `step` may move the play world in these tests.
    fn open(&mut self, player: &mut Player, opened: WindowEvent) {
        self.feed(player, opened);
        self.ok("sim.pause", json!({}));
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
    // Presents carry the window's scale factor (2 here) so the UI draws at display density; the
    // UI hears the size in logical pixels (physical / 2) as `Viewport` input.
    let present = |w: u32, h: u32| json!({"width": w, "height": h, "world": "play", "scale": 2.0});
    let viewport = |w: f32, h: f32| json!({"events": [{"event": {"Viewport": {"width": w, "height": h}}}], "actor": "human:player"});
    assert_eq!(
        calls,
        [
            ("sim.play".to_owned(), json!({})),
            ("input.inject".to_owned(), viewport(640.0, 360.0)),
            ("viewport.present".to_owned(), present(1280, 720)),
            ("input.inject".to_owned(), viewport(400.0, 300.0)),
            ("viewport.present".to_owned(), present(800, 600)),
            ("sim.stop".to_owned(), json!({})),
        ]
    );
}

#[test]
fn window_input_reaches_the_play_world() {
    let mut stage = Stage::new();
    let mut player = Player::new(false);
    stage.open(&mut player, opened(1280, 720, 1.0));
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
    stage.open(&mut player, opened(1920, 1080, 2.0));
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
    stage.open(&mut player, opened(1280, 720, 1.0));
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
    stage.open(&mut player, opened(960, 540, 1.0));
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
    stage.open(&mut player, opened(1280, 720, 1.0));
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

/// A scale change re-sends the logical size (800 × 600 physical at scale 1 after 2: 800 × 600).
/// Each cursor move asks `ui.hit_test` for the hovered widget's cursor; a project without `[ui]`
/// answers "no such method" once and is never asked again.
#[test]
fn scale_changes_resend_the_viewport_and_moves_ask_for_the_cursor() {
    let mut calls: Vec<(String, Value)> = Vec::new();
    let mut cursor = "Pointer";
    let mut player = Player::new(false);
    let feed = |player: &mut Player,
                event: WindowEvent,
                calls: &mut Vec<(String, Value)>,
                cursor: &str| {
        let mut api = |method: &str, params: Value| -> Result<Value, ApiError> {
            calls.push((method.to_owned(), params));
            match method {
                "ui.hit_test" if cursor == "none" => Err(ApiError::new(
                    concordia_api::ApiErrorKind::MethodNotFound,
                    "no method named \"ui.hit_test\"",
                )),
                "ui.hit_test" => Ok(json!({"cursor": cursor})),
                _ => Ok(json!({"origin": [0.0, 0.0, 0.0], "direction": [0.0, -1.0, 0.0]})),
            }
        };
        player.event(&event, &mut api).unwrap();
    };
    feed(&mut player, opened(800, 600, 2.0), &mut calls, cursor);
    calls.clear();
    feed(
        &mut player,
        WindowEvent::ScaleChanged { scale_factor: 1.0 },
        &mut calls,
        cursor,
    );
    assert_eq!(
        calls,
        [(
            "input.inject".to_owned(),
            json!({"events": [{"event": {"Viewport": {"width": 800.0, "height": 600.0}}}], "actor": "human:player"})
        )]
    );
    assert_eq!(player.cursor(), CursorIcon::Arrow);
    let mv = || input(InputEvent::PointerMove { x: 10.0, y: 20.0 });
    feed(&mut player, mv(), &mut calls, cursor);
    assert!(
        calls.iter().any(
            |(m, p)| m == "ui.hit_test" && *p == json!({"x": 10.0, "y": 20.0, "world": "play"})
        )
    );
    assert_eq!(player.cursor(), CursorIcon::Pointer);
    cursor = "Text";
    feed(&mut player, mv(), &mut calls, cursor);
    assert_eq!(player.cursor(), CursorIcon::Text);
    cursor = "none";
    feed(&mut player, mv(), &mut calls, cursor);
    assert_eq!(player.cursor(), CursorIcon::Arrow);
    calls.clear();
    feed(&mut player, mv(), &mut calls, cursor);
    assert!(
        calls.iter().all(|(m, _)| m != "ui.hit_test"),
        "not asked again"
    );
}

/// A clipboard that records what it was given.
#[derive(Default, Clone)]
struct FakeClipboard(std::sync::Arc<std::sync::Mutex<(Option<String>, Vec<String>)>>);

impl concordia_app::Clipboard for FakeClipboard {
    fn get(&mut self) -> Option<String> {
        self.0.lock().unwrap().0.clone()
    }

    fn set(&mut self, text: &str) {
        self.0.lock().unwrap().1.push(text.to_owned());
    }
}

/// The OS clipboard (2.7b.3): a paste injects the OS clipboard's text as typed text (cut to
/// 4096 bytes on a character boundary), or the engine's paste when the OS has none; after
/// input, the player reads `ui.clipboard` and writes a new generation to the OS clipboard
/// once.
#[test]
fn paste_reads_the_os_clipboard_and_copies_reach_it() {
    use concordia_input::{EditKey, TextInput};
    let fake = FakeClipboard::default();
    fake.0.lock().unwrap().0 = Some("from os".into());
    let mut player = Player::with_clipboard(false, Box::new(fake.clone()));
    let mut calls: Vec<(String, Value)> = Vec::new();
    let mut generation = 1;
    let paste = || {
        input(InputEvent::Text(TextInput::Edit {
            key: EditKey::Paste,
            select: false,
        }))
    };
    let feed = |player: &mut Player,
                event: WindowEvent,
                generation: u64,
                calls: &mut Vec<(String, Value)>| {
        let mut api = |method: &str, params: Value| -> Result<Value, ApiError> {
            calls.push((method.to_owned(), params));
            match method {
                "ui.clipboard" => Ok(json!({"text": "copied", "generation": generation})),
                _ => Ok(json!({})),
            }
        };
        player.event(&event, &mut api).unwrap();
    };
    feed(&mut player, paste(), generation, &mut calls);
    let injected = |calls: &[(String, Value)]| {
        calls
            .iter()
            .filter(|(m, _)| m == "input.inject")
            .map(|(_, p)| p["events"][0]["event"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(injected(&calls), [json!({"Text": {"Insert": "from os"}})]);
    assert_eq!(
        fake.0.lock().unwrap().1,
        ["copied"],
        "generation 1 reached the OS"
    );
    calls.clear();
    feed(&mut player, paste(), generation, &mut calls);
    assert_eq!(
        fake.0.lock().unwrap().1.len(),
        1,
        "the same generation is written once"
    );
    generation = 2;
    feed(
        &mut player,
        input(InputEvent::Key {
            key: KeyCode::C,
            pressed: false,
        }),
        generation,
        &mut calls,
    );
    assert_eq!(fake.0.lock().unwrap().1.len(), 2);
    // A long paste is cut to 4096 bytes on a character boundary (é is 2 bytes).
    fake.0.lock().unwrap().0 = Some(format!("a{}", "é".repeat(3000)));
    calls.clear();
    feed(&mut player, paste(), generation, &mut calls);
    let text = injected(&calls)[0]["Text"]["Insert"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(text.len(), 4095);
    // Nothing on the OS clipboard: the engine's own paste goes through.
    fake.0.lock().unwrap().0 = None;
    calls.clear();
    feed(&mut player, paste(), generation, &mut calls);
    assert_eq!(
        injected(&calls),
        [json!({"Text": {"Edit": {"key": "Paste", "select": false}}})]
    );
}
