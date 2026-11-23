//! UI interaction replays exactly (M2 phase 2.7b.2, ADR-0060, ADR-0072): the UI reads the
//! pointer, clicks, wheel, keys and viewport only through the starter map's `ui_*` actions, which
//! replays record like any action, so a recorded session with clicks, a drag, the wheel, Tab and
//! a resize replays to the same hash at every checkpoint, at 1 and at 8 threads.
//!
//! The scene: a 400 × 300 root (stretch disabled) with a toggle at (10, 10)–(110, 34), a slider
//! (0..10, step 1) at (10, 50)–(210, 70), a button anchored to the top right (offsets −100, 10,
//! −10, 40), and a 200 × 100 scroll at (10, 100) over five 50 px rows. The session clicks the
//! toggle (on), drags the slider from x = 110 to 160 ((160 − 18)/184 · 10 = 7.7 → 8), wheels the
//! scroll down 2 lines (80), resizes to 800 × 600 and clicks the button where it moved
//! (745, 25), then Tabs twice.

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use concordia_input::InputMap;
use serde_json::{Value, json};

const ACTOR: &str = "agent:qa";
const MANIFEST: &str = "[input]\nmap = \"input/game.input.ron\"\n\n[ui]\n\n[replay]\n";
const PATH: &str = "tests/replays/ui.replay.ron";

struct Stage {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: concordia_core::id::SessionId,
}

impl Stage {
    fn new(threads: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("concordia.toml"), MANIFEST).unwrap();
        std::fs::create_dir_all(dir.path().join("input")).unwrap();
        std::fs::write(
            dir.path().join("input/game.input.ron"),
            InputMap::starter().to_ron().unwrap(),
        )
        .unwrap();
        let project = Project::load(dir.path()).unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(21);
        config.threads = std::num::NonZeroUsize::new(threads).unwrap();
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut stage = Self {
            _dir: dir,
            engine,
            session,
        };
        stage.ok("session.open", json!({"client": "test", "actor": ACTOR}));
        stage.ok(
            "scene.new",
            json!({"path": "scenes/menu.scene.ron", "name": "Menu"}),
        );
        stage
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    fn spawn(&mut self, components: Value) -> String {
        self.ok("entity.spawn", json!({ "components": components }))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn play(&mut self, id: &str, component: &str) -> Value {
        self.ok(
            "component.get",
            json!({"entity": id, "component": component, "world": "play"}),
        )["value"]
            .clone()
    }
}

fn pinned(parent: &str, [l, t, r, b]: [f32; 4], more: Value) -> Value {
    let mut c = json!({
        "Parent": parent,
        "UiNode": {"position": {"Anchored": {"offsets": {"left": l, "top": t, "right": r, "bottom": b}}}},
    });
    for (k, v) in more.as_object().unwrap() {
        c[k] = v.clone();
    }
    c
}

/// The widgets: toggle, slider, button, scroll.
fn build(stage: &mut Stage) -> [String; 4] {
    let root = stage.spawn(json!({"UiRoot": {"canvas": [400.0, 300.0]}, "UiNode": {}}));
    let toggle = stage.spawn(pinned(
        &root,
        [10.0, 10.0, 110.0, 34.0],
        json!({"UiToggle": {}}),
    ));
    let slider = stage.spawn(pinned(
        &root,
        [10.0, 50.0, 210.0, 70.0],
        json!({"UiSlider": {"max": 10.0, "step": 1.0}}),
    ));
    let button = stage.spawn(json!({
        "Parent": root,
        "UiButton": {},
        "UiNode": {"position": {"Anchored": {
            "anchors": {"left": 1.0, "top": 0.0, "right": 1.0, "bottom": 0.0},
            "offsets": {"left": -100.0, "top": 10.0, "right": -10.0, "bottom": 40.0}
        }}},
    }));
    let scroll = stage.spawn(json!({
        "Parent": root,
        "UiScroll": {},
        "UiNode": {"layout": {"Flex": {"direction": "Column"}}, "position": {"Anchored": {
            "offsets": {"left": 10.0, "top": 100.0, "right": 210.0, "bottom": 200.0}
        }}},
    }));
    for _ in 0..5 {
        stage.spawn(json!({"Parent": scroll, "UiNode": {"height": {"Px": 50.0}}}));
    }
    [toggle, slider, button, scroll]
}

fn session() -> Value {
    let e = |tick: u64, event: Value| json!({"tick": tick, "event": event});
    let at = |x: f32, y: f32| json!({"PointerMove": {"x": x, "y": y}});
    let mouse = |pressed: bool| json!({"MouseButton": {"button": "Left", "pressed": pressed}});
    let key = |k: &str, pressed: bool| json!({"Key": {"key": k, "pressed": pressed}});
    json!([
        e(5, at(60.0, 20.0)),
        e(6, mouse(true)),
        e(8, mouse(false)),
        e(12, at(110.0, 60.0)),
        e(13, mouse(true)),
        e(15, at(160.0, 60.0)),
        e(17, mouse(false)),
        e(20, at(100.0, 150.0)),
        e(21, json!({"Scroll": {"dx": 0.0, "dy": -2.0}})),
        e(30, json!({"Viewport": {"width": 800.0, "height": 600.0}})),
        e(31, at(745.0, 25.0)),
        e(32, mouse(true)),
        e(34, mouse(false)),
        e(40, key("Tab", true)),
        e(41, key("Tab", false)),
        e(42, key("Tab", true)),
        e(43, key("Tab", false)),
    ])
}

/// Records the session for 60 ticks; returns the final hash and the widgets' values.
fn record(threads: usize) -> (Stage, String, Vec<Value>) {
    let mut stage = Stage::new(threads);
    let [toggle, slider, button, scroll] = build(&mut stage);
    stage.ok(
        "replay.record",
        json!({"action": "start", "path": PATH, "checkpoint_every": 10}),
    );
    stage.ok("input.inject", json!({ "events": session() }));
    let hash = stage.ok("sim.step", json!({"ticks": 60}))["world_hash"]
        .as_str()
        .unwrap()
        .to_owned();
    stage.ok("replay.record", json!({"action": "stop"}));
    let values = vec![
        stage.play(&toggle, "UiToggle")["on"].clone(),
        stage.play(&slider, "UiSlider")["value"].clone(),
        stage.play(&scroll, "UiScroll")["offset"].clone(),
        stage.play(&button, "UiState")["focused"].clone(),
        stage.play(&toggle, "UiState")["focused"].clone(),
    ];
    (stage, hash, values)
}

#[test]
fn replay_with_clicks_drags_and_resize_is_exact_at_1_and_8_threads() {
    let (mut one, hash, values) = record(1);
    // The input really drove the UI: toggle on, slider at 8, scrolled 80. Clicking the button
    // focused it; Tab order is toggle, slider, button, so two Tabs wrap to the toggle, then the
    // slider: neither the button nor the toggle ends focused.
    assert_eq!(values[0], true);
    assert_eq!(values[1], 8.0);
    assert_eq!(values[2], json!([0.0, 80.0]));
    assert_eq!(values[3], false);
    assert_eq!(values[4], false);
    one.ok("sim.stop", json!({}));
    let run = one.ok("replay.run", json!({"path": PATH}));
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["matched"], 7, "checkpoints 0, 10, ..., 60: {run}");
    assert_eq!(run["world_hash"], hash.as_str());
    let (_, eight, values8) = record(8);
    assert_eq!(eight, hash, "8 threads");
    assert_eq!(values8, values);
}

/// Typed text replays exactly (2.7b.3): a session clicks a text box at (10, 220)–(210, 250),
/// types "hello", Backspace, " wor" and "ld!", then selects the last character with Shift-← and
/// types "?". Step by step: "hello" → Backspace → "hell" → " wor" → "hell wor" → "ld!" →
/// "hell world!" → Shift-← selects "!" → "?" replaces it: "hell world?". It replays to the same
/// hash, and runs the same at 8 threads.
#[test]
fn text_input_is_recorded_and_replayed() {
    let typing = |threads: usize| {
        let mut stage = Stage::new(threads);
        let root = stage.spawn(json!({"UiRoot": {"canvas": [400.0, 300.0]}, "UiNode": {}}));
        let field = stage.spawn(pinned(
            &root,
            [10.0, 220.0, 210.0, 250.0],
            json!({"UiTextBox": {}}),
        ));
        stage.ok(
            "replay.record",
            json!({"action": "start", "path": PATH, "checkpoint_every": 5}),
        );
        let e = |tick: u64, event: Value| json!({"tick": tick, "event": event});
        let text = |s: &str| json!({"Text": {"Insert": s}});
        let edit = |k: &str, select: bool| json!({"Text": {"Edit": {"key": k, "select": select}}});
        let events = json!([
            e(2, json!({"PointerMove": {"x": 100.0, "y": 235.0}})),
            e(
                3,
                json!({"MouseButton": {"button": "Left", "pressed": true}})
            ),
            e(
                4,
                json!({"MouseButton": {"button": "Left", "pressed": false}})
            ),
            e(6, text("hello")),
            e(7, edit("Backspace", false)),
            e(8, text(" wor")),
            e(9, text("ld!")),
            e(11, edit("Left", true)),
            e(12, text("?")),
        ]);
        stage.ok("input.inject", json!({ "events": events }));
        let hash = stage.ok("sim.step", json!({"ticks": 20}))["world_hash"]
            .as_str()
            .unwrap()
            .to_owned();
        stage.ok("replay.record", json!({"action": "stop"}));
        let typed = stage.play(&field, "UiTextBox")["text"].clone();
        (stage, hash, typed)
    };
    let (mut one, hash, typed) = typing(1);
    assert_eq!(typed, "hell world?");
    one.ok("sim.stop", json!({}));
    let run = one.ok("replay.run", json!({"path": PATH}));
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["world_hash"], hash.as_str());
    let (_, eight, typed8) = typing(8);
    assert_eq!((eight, typed8), (hash, typed));
}
