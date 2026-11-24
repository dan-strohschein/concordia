//! World-space UI takes input (M3 phase 3.0c): the simulation lays world roots out through the
//! play world's camera, so a click on a world button presses it, and `ui.hit_test` and
//! `ui.inspect` see world roots.
//!
//! Derived here: a 400 × 200 viewport (injected as `Viewport`), an orthographic camera at
//! (0, 0, 10) looking down −Z with a visible height of 10 (so 20 wide at aspect 2). A point (x, y, 0)
//! lands at ((x/10 + 1)/2 · 400, (1 − y/5)/2 · 200), so the world root at (3, 1, 0) is centered at
//! (260, 80). Its 100 × 40 canvas at pixel size 1 spans (210, 60)–(310, 100), filled by a button.
//! A click at (260, 80) presses it once; a click at (100, 150) presses nothing. The same clicks
//! give the same world hashes at 1 and 8 threads.

use std::num::NonZeroUsize;

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use concordia_input::InputMap;
use concordia_ui::UiPressed;
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";
const MANIFEST: &str = "[input]\nmap = \"input/game.input.ron\"\n\n[ui]\n\n[render]\n";

struct Stage {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: concordia_core::id::SessionId,
    button: String,
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
        config.deterministic_ids = Some(17);
        config.threads = NonZeroUsize::new(threads).unwrap();
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut stage = Self {
            _dir: dir,
            engine,
            session,
            button: String::new(),
        };
        stage.ok(
            "scene.new",
            json!({"path": "scenes/w.scene.ron", "name": "W", "actor": ACTOR}),
        );
        stage.spawn(json!({
            "Transform": place([0.0, 0.0, 10.0]),
            "Camera": {"projection": {"Orthographic": {"height": 10.0, "near": 0.1, "far": 100.0}}},
        }));
        let root = stage.spawn(json!({
            "Transform": place([3.0, 1.0, 0.0]),
            "UiRoot": {"canvas": [100.0, 40.0], "space": {"World": {"offset": [0.0, 0.0, 0.0], "pixel_size": 1.0}}},
            "UiNode": {},
        }));
        stage.button = stage.spawn(json!({
            "Parent": root,
            "UiButton": {},
            "UiNode": {"width": {"Percent": 100.0}, "height": {"Percent": 100.0}},
        }));
        stage.tick(vec![json!({"Viewport": {"width": 400.0, "height": 200.0}})]);
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

    /// Steps one tick after injecting `events`; returns the world hash.
    fn tick(&mut self, events: Vec<Value>) -> String {
        let events: Vec<Value> = events.into_iter().map(|e| json!({ "event": e })).collect();
        if !events.is_empty() {
            self.ok("input.inject", json!({ "events": events }));
        }
        self.ok("sim.step", json!({"ticks": 1}))["world_hash"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// Clicks at (x, y): move and press, then release; returns the hash after the release.
    fn click(&mut self, x: f32, y: f32) -> String {
        self.tick(vec![
            json!({"PointerMove": {"x": x, "y": y}}),
            json!({"MouseButton": {"button": "Left", "pressed": true}}),
        ]);
        self.tick(vec![
            json!({"MouseButton": {"button": "Left", "pressed": false}}),
        ])
    }

    fn pressed(&mut self) -> usize {
        let world = self.engine.play_world_mut().unwrap();
        world.pending_signals::<UiPressed>().len()
    }
}

fn place(p: [f32; 3]) -> Value {
    json!({"translation": p, "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]})
}

#[test]
fn world_button_is_pressed_by_a_click_at_its_projected_centre() {
    let mut stage = Stage::new(1);
    stage.click(100.0, 150.0);
    assert_eq!(stage.pressed(), 0, "a click off the button");
    stage.click(260.0, 80.0);
    assert_eq!(stage.pressed(), 1, "a click on the world button");
}

#[test]
fn hit_test_and_inspect_see_world_roots() {
    let mut stage = Stage::new(1);
    let hit = stage.ok(
        "ui.hit_test",
        json!({"x": 260.0, "y": 80.0, "world": "play"}),
    );
    assert_eq!(hit["widget"], stage.button.as_str(), "{hit}");
    let report = stage.ok("ui.inspect", json!({"world": "play"}));
    let node = report["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["entity"] == stage.button.as_str())
        .unwrap_or_else(|| panic!("the world button is reported: {report}"));
    // Node boxes are in the root's UI space; its map places that space on the screen.
    let r = &node["rect"];
    let f = |k: &str| r[k].as_f64().unwrap();
    assert_eq!((f("x"), f("y"), f("w"), f("h")), (0.0, 0.0, 100.0, 40.0));
    let root = report["roots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["entity"] == node["root"])
        .unwrap();
    assert_eq!(root["map"]["offset"], json!([210.0, 60.0]), "{root}");
}

#[test]
fn world_clicks_are_deterministic_across_threads() {
    let mut hashes = Vec::new();
    for threads in [1, 8] {
        let mut stage = Stage::new(threads);
        stage.click(260.0, 80.0);
        hashes.push(stage.click(100.0, 150.0));
    }
    assert_eq!(hashes[0], hashes[1]);
}
