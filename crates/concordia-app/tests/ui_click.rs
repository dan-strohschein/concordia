//! The click goes to the UI first (M2 phase 2.7b.2, ADR-0060): `ui.interact` runs after input
//! sampling and before `nav.click`, and marks the pointer consumed when it is over the UI, so a
//! click on a HUD button never moves the hero; a click elsewhere still does.
//!
//! A 960 × 540 root (no viewport input, so the viewport is its canvas) holds a button at
//! (0, 0)–(200, 100). The floor's top is y = 0, walkable to ±9.5 (the nav tests' level). A ray
//! straight down at (3, −4) hits (3, 0, −4) and snaps to (3, 0.2, −4). With the pointer at
//! (100, 50) the click is the button's: `UiPressed` fires on release and the hero's target stays
//! unset. With the pointer at (500, 300), off the UI, the same click sets the target.

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use concordia_input::InputMap;
use concordia_ui::UiPressed;
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";
const MANIFEST: &str = "[input]\nmap = \"input/game.input.ron\"\n\n[physics]\n\n[nav]\n\n[ui]\n";

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
        std::fs::write(
            dir.path().join("input/game.input.ron"),
            InputMap::starter().to_ron().unwrap(),
        )
        .unwrap();
        let project = Project::load(dir.path()).unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(13);
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
            json!({"path": "scenes/c.scene.ron", "name": "C", "actor": ACTOR}),
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

    fn tick(&mut self, events: Vec<Value>) {
        let events: Vec<Value> = events.into_iter().map(|e| json!({ "event": e })).collect();
        if !events.is_empty() {
            self.ok("input.inject", json!({ "events": events }));
        }
        self.ok("sim.step", json!({"ticks": 1}));
    }

    fn target(&mut self, hero: &str) -> Value {
        self.ok(
            "component.get",
            json!({"entity": hero, "component": "NavAgent", "world": "play"}),
        )["value"]["target"]
            .clone()
    }

    fn pressed(&mut self) -> usize {
        let world = self.engine.play_world_mut().unwrap();
        world.pending_signals::<UiPressed>().len()
    }
}

fn place(p: [f32; 3]) -> Value {
    json!({"translation": p, "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]})
}

fn click_at(stage: &mut Stage, x: f32, y: f32) {
    stage.tick(vec![
        json!({"PointerMove": {"x": x, "y": y}}),
        json!({"PointerRay": {"origin": [3.0, 10.0, -4.0], "direction": [0.0, -1.0, 0.0]}}),
        json!({"MouseButton": {"button": "Left", "pressed": true}}),
    ]);
}

fn level() -> (Stage, String) {
    let mut stage = Stage::new();
    stage.spawn(json!({
        "Transform": place([0.0, -0.5, 0.0]),
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [10.1, 0.5, 10.1]}}},
    }));
    stage.spawn(json!({
        "Transform": place([0.0, 0.0, 0.0]),
        "NavMeshVolume": {"half_extents": [12.0, 3.0, 12.0]},
    }));
    let hero = stage.spawn(json!({
        "Transform": place([-5.0, 0.0, 0.0]),
        "CharacterController": {},
        "CharacterMotion": {},
        "NavAgent": {"max_speed": 3.0},
        "ClickToMove": {},
    }));
    let root = stage.spawn(json!({"UiRoot": {"canvas": [960.0, 540.0]}, "UiNode": {}}));
    stage.spawn(json!({
        "Parent": root,
        "UiButton": {},
        "UiNode": {"position": {"Anchored": {"offsets": {"left": 0.0, "top": 0.0, "right": 200.0, "bottom": 100.0}}}},
    }));
    stage.tick(vec![]);
    (stage, hero)
}

#[test]
fn pointer_over_ui_consumes_click_before_world() {
    let (mut stage, hero) = level();
    click_at(&mut stage, 100.0, 50.0);
    assert_eq!(
        stage.target(&hero),
        Value::Null,
        "the button took the click"
    );
    stage.tick(vec![
        json!({"MouseButton": {"button": "Left", "pressed": false}}),
    ]);
    assert_eq!(stage.pressed(), 1);
    assert_eq!(stage.target(&hero), Value::Null);
}

#[test]
fn click_outside_ui_still_moves() {
    let (mut stage, hero) = level();
    click_at(&mut stage, 500.0, 300.0);
    let t = stage.target(&hero);
    let want = [3.0, 0.2, -4.0];
    for (i, w) in want.iter().enumerate() {
        assert!((t[i].as_f64().unwrap() - w).abs() < 1e-3, "{t}");
    }
}
