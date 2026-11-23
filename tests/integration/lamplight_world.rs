//! The Lamplight town in the engine for integration tests: a fresh copy of the example with
//! both gameplay modules built, driven through the Engine API (shared by `lamplight_night` and
//! `lamplight_town`).

#![allow(
    dead_code,
    reason = "shared by two test binaries; each uses only some helpers"
)]

use concordia_api::{Engine, EngineConfig};
use concordia_app::Project;
use serde_json::{Value, json};

pub const ACTOR: &str = "agent:test";

pub struct Night {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: concordia_core::id::SessionId,
}

impl Night {
    /// The town on a fresh copy of the example, with both modules built, the night `phase`.
    pub fn new(phase: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        concordia_tests::copy_example("m2-lamplight", root).unwrap();
        std::fs::create_dir_all(root.join("build")).unwrap();
        let rules =
            concordia_tests::wasm_fixture_path("lamplight_rules").unwrap_or_else(|e| panic!("{e}"));
        std::fs::copy(rules, root.join("build/lamplight_rules.wasm")).unwrap();
        let fog =
            concordia_tests::native_fixture_path("lamplight_fog").unwrap_or_else(|e| panic!("{e}"));
        let fog_file = format!("build/lamplight_fog.{}", std::env::consts::DLL_EXTENSION);
        std::fs::copy(fog, root.join(fog_file)).unwrap();
        let project = Project::load(root).unwrap();
        let mut config = EngineConfig::new(root);
        config.deterministic_ids = Some(5);
        config.modules = project.modules().unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut night = Self {
            _dir: dir,
            engine,
            session,
        };
        night.ok("scene.open", json!({"path": "scenes/town.scene.ron"}));
        let id = night.one("lamplight.Night", "edit").0;
        night.patch(&id, "lamplight.Night", json!({"phase": phase}));
        night
    }

    pub fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    /// Every entity with `component` in `world`: (id, its components), page by page.
    pub fn all(&mut self, component: &str, world: &str) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        let mut cursor = Value::Null;
        loop {
            let page = self.ok(
                "query.run",
                json!({"with": [component], "world": world, "limit": 1000, "cursor": cursor}),
            );
            for r in page["rows"].as_array().unwrap() {
                out.push((
                    r["id"].as_str().unwrap().to_owned(),
                    r["components"].clone(),
                ));
            }
            cursor = page["next_cursor"].clone();
            if cursor.is_null() {
                return out;
            }
        }
    }

    pub fn one(&mut self, component: &str, world: &str) -> (String, Value) {
        let mut all = self.all(component, world);
        assert_eq!(all.len(), 1, "one {component} in {world}");
        all.remove(0)
    }

    /// The entity named `name` in the edit world.
    pub fn named(&mut self, name: &str) -> (String, Value) {
        let rows = self.all("Name", "edit");
        rows.into_iter()
            .find(|(_, c)| c["Name"] == name)
            .unwrap_or_else(|| panic!("no entity named {name}"))
    }

    pub fn patch(&mut self, entity: &str, component: &str, patch: Value) {
        self.ok(
            "component.patch",
            json!({"entity": entity, "component": component, "patch": patch, "actor": ACTOR}),
        );
    }

    pub fn play(&mut self, entity: &str, component: &str) -> Value {
        self.ok(
            "component.get",
            json!({"entity": entity, "component": component, "world": "play"}),
        )["value"]
            .clone()
    }

    pub fn step(&mut self, ticks: u32) -> String {
        self.ok("sim.step", json!({"ticks": ticks}))["world_hash"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub fn keys(&mut self, events: &[(u64, &str, bool)]) {
        let events: Vec<Value> = events
            .iter()
            .map(|(tick, key, pressed)| json!({"tick": tick, "event": {"Key": {"key": key, "pressed": pressed}}}))
            .collect();
        self.ok("input.inject", json!({"events": events, "actor": ACTOR}));
    }

    /// Puts the lamplighter at `at` in the edit world.
    pub fn place_lamplighter(&mut self, at: [f64; 3]) -> String {
        let walker = self.one("lamplight.Lamplighter", "edit").0;
        self.patch(&walker, "Transform", json!({"translation": at}));
        walker
    }

    /// A lamp's light (`Lamp <index>`) and its ground position.
    pub fn lamp(&mut self, index: u32) -> (String, [f64; 3]) {
        let (id, components) = self.named(&format!("Lamp {index}"));
        let t = &components["Transform"]["translation"];
        (id, [t[0].as_f64().unwrap(), 0.0, t[2].as_f64().unwrap()])
    }

    pub fn light_lamp(&mut self, lamp: &str, oil: f64) {
        self.patch(lamp, "lamplight.Lamp", json!({"lit": true, "oil": oil}));
        self.patch(lamp, "PointLight", json!({"intensity_lumens": 2400.0}));
    }

    pub fn wisp_at(&mut self, at: [f64; 3]) -> String {
        self.ok(
            "entity.create",
            json!({"template": "wisp", "translation": at, "actor": ACTOR}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// A wisp at `at` drifting to the ring point 4.5 m south of the square.
    pub fn wisp_for_the_square(&mut self, at: [f64; 3]) -> String {
        let wisp = self.wisp_at(at);
        self.patch(&wisp, "fog.Drifter", json!({"target": [0.0, 0.0, 4.5]}));
        wisp
    }

    pub fn night(&mut self) -> Value {
        self.one("lamplight.Night", "play").1["lamplight.Night"].clone()
    }

    pub fn errors(&mut self) -> Value {
        self.ok("log.tail", json!({"level": "error"}))["records"].clone()
    }
}

pub fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}
