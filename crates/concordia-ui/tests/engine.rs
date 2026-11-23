//! The `[ui]` engine module: registration, edits with an actor, scenes, `ui.inspect`, tweens on
//! UI fields, the draw hook, and the determinism boundary (layout and drawing never change a
//! world hash, at 1 or 8 threads).

use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_api::{Engine, EngineConfig, EngineModule, UiFrame, WorldSel};
use concordia_assets::AssetsModule;
use concordia_core::id::SessionId;
use concordia_ui::{UI_BOUNDS_LAYER, UiModule};
use serde_json::{Value, json};

const ACTOR: &str = "agent:test";

struct Rig {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
}

impl Rig {
    fn new(threads: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(5);
        config.threads = NonZeroUsize::new(threads).unwrap();
        config.journal.sync = false;
        let assets = concordia_ui::importers()
            .into_iter()
            .fold(AssetsModule::new(dir.path()), AssetsModule::with_importer);
        config.modules = vec![
            Arc::new(assets),
            Arc::new(concordia_anim::AnimModule::new()),
            Arc::new(UiModule::new()),
        ];
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut rig = Self {
            _dir: dir,
            engine,
            session,
        };
        rig.ok("session.open", json!({"client": "test", "actor": ACTOR}));
        rig.ok(
            "scene.new",
            json!({"path": "scenes/hud.scene.ron", "name": "HUD"}),
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

    /// A 400 × 300 root with a label pinned at (20, 30, 200 × 40) and a bar under it.
    fn hud(&mut self) -> (String, String, String) {
        let root = self.spawn(json!({"UiRoot": {"canvas": [400.0, 300.0]}, "UiNode": {}}));
        let label = self.spawn(json!({
            "Parent": root,
            "UiNode": {"position": {"Anchored": {"offsets": {"left": 20.0, "top": 30.0, "right": 220.0, "bottom": 70.0}}}},
            "UiText": {"runs": [{"text": "Hello"}]},
        }));
        let bar = self.spawn(json!({
            "Parent": root,
            "UiNode": {"position": {"Anchored": {"offsets": {"left": 20.0, "top": 80.0, "right": 220.0, "bottom": 94.0}}}},
            "UiProgress": {"value": 40.0},
            "UiStyle": {"opacity": 1.0},
        }));
        (root, label, bar)
    }
}

#[test]
fn ui_module_registers_its_method_and_debug_layer() {
    let module = UiModule::new();
    let names: Vec<_> = module
        .methods()
        .iter()
        .map(|m| (m.name, m.mutates))
        .collect();
    assert_eq!(names, [("ui.inspect", false), ("ui.hit_test", false)]);
    assert_eq!(module.debug_layers(), [UI_BOUNDS_LAYER]);
    assert_eq!(UI_BOUNDS_LAYER, "ui_bounds");
}

/// Every UI edit is an ordinary journaled command: three spawns and a `component.set`, each a
/// commit by `agent:test`.
#[test]
fn ui_edits_go_through_commands_with_actor() {
    let mut rig = Rig::new(1);
    let (_, label, _) = rig.hud();
    rig.ok(
        "component.set",
        json!({"entity": label, "component": "UiText", "value": {"runs": [{"text": "Bye"}]}}),
    );
    let history = rig.ok("txn.history", json!({}));
    let commits: Vec<&Value> = history["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "commit")
        .collect();
    assert_eq!(commits.len(), 4);
    assert!(commits.iter().all(|e| e["actor"] == ACTOR), "{history}");
    // A bad value is refused at the boundary, naming the field.
    let err = rig
        .call(
            "component.set",
            json!({"entity": label, "component": "UiText", "value": {"line_height": 9.0}}),
        )
        .unwrap_err();
    assert_eq!(err.data.kind, "ValueInvalid", "{err:?}");
    assert_eq!(err.data.pointer.as_deref(), Some("/line_height"));
}

/// Saved and reopened, every UI component reads back the same.
#[test]
fn ui_scene_round_trips_through_scene_ron() {
    let mut rig = Rig::new(1);
    let (root, label, bar) = rig.hud();
    let get = |rig: &mut Rig, id: &str, c: &str| {
        rig.ok("component.get", json!({"entity": id, "component": c}))["value"].clone()
    };
    let before: Vec<Value> = [
        (&root, "UiRoot"),
        (&label, "UiNode"),
        (&label, "UiText"),
        (&bar, "UiProgress"),
        (&bar, "UiStyle"),
    ]
    .iter()
    .map(|(id, c)| get(&mut rig, id, c))
    .collect();
    rig.ok("scene.save", json!({}));
    rig.ok("scene.open", json!({"path": "scenes/hud.scene.ron"}));
    let after: Vec<Value> = [
        (&root, "UiRoot"),
        (&label, "UiNode"),
        (&label, "UiText"),
        (&bar, "UiProgress"),
        (&bar, "UiStyle"),
    ]
    .iter()
    .map(|(id, c)| get(&mut rig, id, c))
    .collect();
    assert_eq!(before, after);
}

/// `ui.inspect` at the canvas size: the root fills 400 × 300, the label sits at its offsets with
/// one line "Hello", the bar below. A 0-wide viewport is refused.
#[test]
fn ui_inspect_reports_rects_and_text_lines() {
    let mut rig = Rig::new(1);
    let (root, label, bar) = rig.hud();
    let r = rig.ok("ui.inspect", json!({}));
    assert_eq!(r["viewport"], json!([400.0, 300.0]));
    let node = |id: &str| {
        r["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["entity"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(
        node(&root)["rect"],
        json!({"x": 0.0, "y": 0.0, "w": 400.0, "h": 300.0})
    );
    assert_eq!(
        node(&label)["rect"],
        json!({"x": 20.0, "y": 30.0, "w": 200.0, "h": 40.0})
    );
    assert_eq!(node(&label)["lines"][0]["text"], "Hello");
    assert_eq!(
        node(&bar)["rect"],
        json!({"x": 20.0, "y": 80.0, "w": 200.0, "h": 14.0})
    );
    // One entity's subtree only.
    let one = rig.ok("ui.inspect", json!({"entity": label}));
    assert_eq!(one["nodes"].as_array().unwrap().len(), 1);
    // A viewport twice the size doesn't move Disabled-stretch nodes.
    let wide = rig.ok("ui.inspect", json!({"width": 800, "height": 600}));
    assert_eq!(wide["viewport"], json!([800.0, 600.0]));
    let err = rig
        .call("ui.inspect", json!({"width": 0, "height": 300}))
        .unwrap_err();
    assert_eq!(err.data.kind, "InvalidParams");
    assert_eq!(err.data.pointer.as_deref(), Some("/width"), "{err:?}");
    // Problems are reported, not fatal.
    rig.spawn(
        json!({"Parent": root, "UiNode": {"min_width": {"Px": 9.0}, "max_width": {"Px": 1.0}}}),
    );
    let r = rig.ok("ui.inspect", json!({}));
    assert_eq!(r["problems"].as_array().unwrap().len(), 1, "{r}");
}

/// `Tweens` (2.4.4) animate UI fields like any component's: opacity 1 → 0 over 1 s, linear, is
/// 0.5 after 30 ticks at 60 Hz.
#[test]
fn tweens_animate_ui_style_opacity() {
    let mut rig = Rig::new(1);
    let (_, _, bar) = rig.hud();
    rig.ok(
        "component.set",
        json!({"entity": bar, "component": "Tweens", "value": {"tweens": [{"component": "UiStyle", "field": "/opacity", "to": [0.0], "duration": 1.0}]}}),
    );
    rig.ok("sim.step", json!({"ticks": 30}));
    let style = rig.ok(
        "component.get",
        json!({"entity": bar, "component": "UiStyle", "world": "play"}),
    )["value"]
        .clone();
    let opacity = style["opacity"].as_f64().unwrap();
    assert!((opacity - 0.5).abs() < 1e-4, "{style}");
}

/// The UI is presentation: stepping with the UI drawn (and inspected) after every tick gives the
/// same world hashes as stepping without, at 1 and 8 threads.
#[test]
fn ui_never_changes_the_world_hash_at_1_and_8_threads() {
    let run = |threads: usize, draw: bool| -> Vec<Value> {
        let mut rig = Rig::new(threads);
        let (_, _, bar) = rig.hud();
        rig.ok(
            "component.set",
            json!({"entity": bar, "component": "Tweens", "value": {"tweens": [{"component": "UiProgress", "field": "/value", "to": [100.0], "duration": 0.5}]}}),
        );
        (0..20)
            .map(|_| {
                let hash = rig.ok("sim.step", json!({"ticks": 3}))["world_hash"].clone();
                if draw {
                    let list = rig.engine.ui_draw(
                        WorldSel::play,
                        &UiFrame {
                            bounds: true,
                            ..UiFrame::new(400, 300)
                        },
                    );
                    assert!(!list.is_empty());
                    rig.ok("ui.inspect", json!({"world": "play"}));
                }
                hash
            })
            .collect()
    };
    let plain = run(1, false);
    assert_eq!(run(1, true), plain);
    assert_eq!(run(8, true), plain);
}
