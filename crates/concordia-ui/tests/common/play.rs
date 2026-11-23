//! A play-world rig: assets, input (the starter map) and UI modules, input injected through
//! `input.inject` and read back through `component.get` and the play world's signals.
#![allow(dead_code)]

use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_api::{Engine, EngineConfig, EngineModule};
use concordia_assets::AssetsModule;
use concordia_core::id::{EntityId, SessionId};
use concordia_input::{InputMap, InputModule};
use concordia_terra::Component;
use concordia_ui::UiModule;
use serde_json::{Value, json};

pub struct Play {
    _dir: tempfile::TempDir,
    pub engine: Engine,
    session: SessionId,
}

impl Play {
    pub fn new(threads: usize) -> Self {
        Self::with_modules(threads, Vec::new())
    }

    /// Extra modules go after input and before UI.
    pub fn with_modules(threads: usize, extra: Vec<Arc<dyn EngineModule>>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.deterministic_ids = Some(5);
        config.threads = NonZeroUsize::new(threads).unwrap();
        config.journal.sync = false;
        let assets = concordia_ui::importers()
            .into_iter()
            .fold(AssetsModule::new(dir.path()), AssetsModule::with_importer);
        let mut modules: Vec<Arc<dyn EngineModule>> = vec![
            Arc::new(InputModule::new(InputMap::starter())),
            Arc::new(assets),
        ];
        modules.extend(extra);
        modules.push(Arc::new(UiModule::new()));
        config.modules = modules;
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut play = Self {
            _dir: dir,
            engine,
            session,
        };
        play.ok(
            "session.open",
            json!({"client": "test", "actor": "agent:test"}),
        );
        play.ok(
            "scene.new",
            json!({"path": "scenes/menu.scene.ron", "name": "Menu"}),
        );
        play
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, concordia_api::ApiError> {
        self.engine.call(self.session, method, params)
    }

    pub fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    pub fn spawn(&mut self, components: Value) -> String {
        self.ok("entity.spawn", json!({ "components": components }))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// A 400 × 300 root (stretch disabled).
    pub fn root(&mut self) -> String {
        self.spawn(json!({"UiRoot": {"canvas": [400.0, 300.0]}, "UiNode": {}}))
    }

    /// A child of `parent` pinned to (l, t)–(r, b), with more components.
    pub fn pinned(&mut self, parent: &str, ltrb: [f32; 4], more: Value) -> String {
        let [l, t, r, b] = ltrb;
        let mut c = json!({
            "Parent": parent,
            "UiNode": {"position": {"Anchored": {"offsets": {"left": l, "top": t, "right": r, "bottom": b}}}},
        });
        for (k, v) in more.as_object().unwrap() {
            c[k] = v.clone();
        }
        self.spawn(c)
    }

    /// Queues events for the next tick, then steps it; returns the world hash.
    pub fn tick(&mut self, events: Vec<Value>) -> String {
        if !events.is_empty() {
            let events: Vec<Value> = events.into_iter().map(|e| json!({ "event": e })).collect();
            self.ok("input.inject", json!({ "events": events }));
        }
        self.ok("sim.step", json!({"ticks": 1}))["world_hash"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// A play-world component (`Null` when absent).
    pub fn get(&mut self, entity: &str, component: &str) -> Value {
        self.call(
            "component.get",
            json!({"entity": entity, "component": component, "world": "play"}),
        )
        .map(|v| v["value"].clone())
        .unwrap_or(Value::Null)
    }

    /// Signals of type `S` raised last tick, as (entity id string, signal).
    pub fn signals<S: Component + Clone>(&mut self) -> Vec<(String, S)> {
        let world = self.engine.play_world_mut().unwrap();
        world
            .pending_signals::<S>()
            .into_iter()
            .map(|(id, s): (EntityId, &S)| {
                (
                    serde_json::to_value(id)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    s.clone(),
                )
            })
            .collect()
    }

    /// A play-world resource.
    pub fn resource<R: concordia_terra::Resource + Clone>(&mut self) -> Option<R> {
        self.engine.play_world_mut().ok()?.resource::<R>().cloned()
    }
}

pub fn at(x: f32, y: f32) -> Value {
    json!({"PointerMove": {"x": x, "y": y}})
}

pub fn mouse(pressed: bool) -> Value {
    json!({"MouseButton": {"button": "Left", "pressed": pressed}})
}

pub fn key(name: &str, pressed: bool) -> Value {
    json!({"Key": {"key": name, "pressed": pressed}})
}

pub fn wheel(dy: f32) -> Value {
    json!({"Scroll": {"dx": 0.0, "dy": dy}})
}

pub fn viewport(width: f32, height: f32) -> Value {
    json!({"Viewport": {"width": width, "height": height}})
}

/// Presses then releases `name` over two ticks.
pub fn tap(play: &mut Play, name: &str) {
    play.tick(vec![key(name, true)]);
    play.tick(vec![key(name, false)]);
}

/// Moves to (x, y) and clicks: press on one tick, release on the next.
pub fn click(play: &mut Play, x: f32, y: f32) {
    play.tick(vec![at(x, y), mouse(true)]);
    play.tick(vec![mouse(false)]);
}
