//! Lamplight over MCP stdio for the M2 gate: a fresh copy of the example with both modules built,
//! a real `concordia-mcp` server, and the player's hands (`input_inject`, `sim_step`, `query_run`).
//! Every tool the gate calls is recorded, for the every-role check.

#![allow(dead_code, reason = "each gate test uses only some helpers")]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use concordia_tests::bins::concordia_mcp_bin;
use concordia_tests::mcp_client::{McpClient, ToolError};
use serde_json::{Value, json};

/// The gate's actor.
pub const ACTOR: &str = "agent:gate";
/// The lamplighter's walking speed (m/s), from the rules module.
const WALK: f64 = 3.5;

/// A copy of Lamplight with `build/` filled from the fixture builds.
pub fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    concordia_tests::copy_example("m2-lamplight", root).unwrap();
    std::fs::create_dir_all(root.join("build")).unwrap();
    let rules =
        concordia_tests::wasm_fixture_path("lamplight_rules").unwrap_or_else(|e| panic!("{e}"));
    std::fs::copy(rules, root.join("build/lamplight_rules.wasm")).unwrap();
    let fog =
        concordia_tests::native_fixture_path("lamplight_fog").unwrap_or_else(|e| panic!("{e}"));
    let file = format!("build/lamplight_fog.{}", std::env::consts::DLL_EXTENSION);
    std::fs::copy(fog, root.join(file)).unwrap();
    dir
}

/// The town on a server.
pub struct Town {
    pub client: McpClient,
    pub root: PathBuf,
    /// The next tick to simulate.
    pub tick: u64,
    called: Mutex<BTreeSet<String>>,
}

impl Town {
    /// A server on `root` with `threads` job threads, the town scene open.
    pub async fn open(root: &Path, threads: usize) -> Self {
        let threads = threads.to_string();
        let args = [
            "--project",
            root.to_str().unwrap(),
            "--actor",
            ACTOR,
            "--deterministic-ids",
            "5",
            "--threads",
            &threads,
        ];
        let client = McpClient::spawn(&concordia_mcp_bin(), &args)
            .await
            .expect("server starts");
        let town = Self {
            client,
            root: root.to_path_buf(),
            tick: 0,
            called: Mutex::new(BTreeSet::new()),
        };
        town.ok("scene_open", json!({"path": "scenes/town.scene.ron"}))
            .await;
        town
    }

    /// Calls `tool`, recording it.
    pub async fn call(&self, tool: &str, args: Value) -> Result<Value, ToolError> {
        self.called.lock().unwrap().insert(tool.to_owned());
        self.client.call(tool, args).await
    }

    pub async fn ok(&self, tool: &str, args: Value) -> Value {
        self.call(tool, args.clone())
            .await
            .unwrap_or_else(|e| panic!("{tool} {args}: {}", e.text))
    }

    /// The tools called so far.
    pub fn called(&self) -> BTreeSet<String> {
        self.called.lock().unwrap().clone()
    }

    /// Steps `ticks`; returns the world hash.
    pub async fn step(&mut self, ticks: u64) -> String {
        self.tick += ticks;
        self.ok("sim_step", json!({"ticks": ticks})).await["world_hash"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// Every entity with `component` in `world`: (id, components).
    pub async fn all(&self, component: &str, world: &str) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        let mut cursor = Value::Null;
        loop {
            let page = self
                .ok(
                    "query_run",
                    json!({"with": [component], "world": world, "limit": 1000, "cursor": cursor}),
                )
                .await;
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

    pub async fn one(&self, component: &str, world: &str) -> (String, Value) {
        let mut all = self.all(component, world).await;
        assert_eq!(all.len(), 1, "one {component} in {world}");
        all.remove(0)
    }

    /// The entity named `name` in `world`.
    pub async fn named(&self, name: &str, world: &str) -> (String, Value) {
        self.all("Name", world)
            .await
            .into_iter()
            .find(|(_, c)| c["Name"] == name)
            .unwrap_or_else(|| panic!("no entity named {name}"))
    }

    /// The night's state in the play world.
    pub async fn night(&self) -> Value {
        self.one("lamplight.Night", "play").await.1["lamplight.Night"].clone()
    }

    /// The lamplighter's state and ground position in the play world.
    pub async fn lamplighter(&self) -> (Value, [f64; 2]) {
        let (_, c) = self.one("lamplight.Lamplighter", "play").await;
        let t = &c["Transform"]["translation"];
        (
            c["lamplight.Lamplighter"].clone(),
            [t[0].as_f64().unwrap(), t[2].as_f64().unwrap()],
        )
    }

    async fn inject(&self, events: Vec<Value>) {
        self.ok("input_inject", json!({"events": events})).await;
    }

    /// Clicks a widget (by name) at its centre in the play world.
    pub async fn click(&mut self, name: &str) {
        let id = self.named(name, "play").await.0;
        let report = self
            .ok("ui_inspect", json!({"world": "play", "entity": id}))
            .await;
        let rect = &report["nodes"][0]["rect"];
        let f = |k: &str| rect[k].as_f64().unwrap();
        let (x, y) = (f("x") + f("w") / 2.0, f("y") + f("h") / 2.0);
        let t = self.tick;
        self.inject(vec![
            json!({"tick": t, "event": {"PointerMove": {"x": x, "y": y}}}),
            json!({"tick": t, "event": {"MouseButton": {"button": "Left", "pressed": true}}}),
            json!({"tick": t + 1, "event": {"MouseButton": {"button": "Left", "pressed": false}}}),
        ])
        .await;
        self.step(3).await;
    }

    /// Types `text` (keys and text, as the OS sends both).
    pub async fn type_text(&mut self, text: &str) {
        let t = self.tick;
        let mut events = Vec::new();
        for c in text.chars() {
            let key = c.to_ascii_uppercase().to_string();
            events.push(json!({"tick": t, "event": {"Key": {"key": key, "pressed": true}}}));
            events.push(json!({"tick": t, "event": {"Text": {"Insert": c.to_string()}}}));
        }
        for c in text.chars() {
            let key = c.to_ascii_uppercase().to_string();
            events.push(json!({"tick": t + 1, "event": {"Key": {"key": key, "pressed": false}}}));
        }
        self.inject(events).await;
        self.step(2).await;
    }

    /// Holds `key` for `ticks`, then releases it and steps one more tick.
    pub async fn hold(&mut self, key: &str, ticks: u64) {
        let t = self.tick;
        self.inject(vec![
            json!({"tick": t, "event": {"Key": {"key": key, "pressed": true}}}),
            json!({"tick": t + ticks, "event": {"Key": {"key": key, "pressed": false}}}),
        ])
        .await;
        self.step(ticks + 1).await;
    }

    pub async fn tap(&mut self, key: &str) {
        self.hold(key, 1).await;
    }

    /// Walks to within `tolerance` of `to` (x, z), one axis at a time, like a player correcting
    /// course; gives up after 12 legs.
    pub async fn walk_to(&mut self, to: [f64; 2], tolerance: f64) -> [f64; 2] {
        for _ in 0..12 {
            let at = self.lamplighter().await.1;
            let (dx, dz) = (to[0] - at[0], to[1] - at[1]);
            if dx.abs() <= tolerance && dz.abs() <= tolerance {
                return at;
            }
            let (key, d) = if dx.abs() > dz.abs() {
                (if dx > 0.0 { "D" } else { "A" }, dx.abs())
            } else {
                (if dz > 0.0 { "S" } else { "W" }, dz.abs())
            };
            let ticks = ((d / WALK * 60.0).round() as u64).max(1);
            self.hold(key, ticks).await;
        }
        self.lamplighter().await.1
    }

    /// A lamp's light (`Lamp <index>`) and its post's ground position.
    pub async fn lamp(&self, index: u32) -> (String, [f64; 2]) {
        let (id, c) = self.named(&format!("Lamp {index}"), "edit").await;
        let t = &c["Transform"]["translation"];
        (id, [t[0].as_f64().unwrap(), t[2].as_f64().unwrap()])
    }

    /// Error log records so far.
    pub async fn errors(&self) -> Value {
        self.ok("log_tail", json!({"level": "error"})).await["records"].clone()
    }
}
