//! The scripts engine module (M2 phase 2.6d): modules load with the engine, run in play, and
//! reload over the Engine API, migrating changed types in both worlds.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use concordia_api::{ApiErrorKind, Engine, EngineConfig, WorldSel};
use concordia_core::id::SessionId;
use concordia_core::log::LogRing;
use concordia_scripting::{ScriptEntry, ScriptKind, ScriptsConfig, ScriptsModule};
use serde_json::{Value, json};
use tracing_subscriber::layer::SubscriberExt;

/// An engine on a temporary project, with a session opened as `agent:test` and a new scene.
struct Project {
    dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
}

fn wasm_fixture(name: &str) -> PathBuf {
    concordia_tests::wasm_fixture_path(name).unwrap_or_else(|e| panic!("{e}"))
}

fn native_fixture(name: &str) -> PathBuf {
    concordia_tests::native_fixture_path(name).unwrap_or_else(|e| panic!("{e}"))
}

/// Copies `from` to `dir/rel` (creating folders).
fn place(dir: &Path, rel: &str, from: &Path) {
    let to = dir.join(rel);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::copy(from, to).unwrap();
}

fn entry(dir: &Path, kind: ScriptKind, rel: &str) -> ScriptEntry {
    ScriptEntry {
        kind,
        path: dir.join(rel),
        source: None,
    }
}

impl Project {
    /// A project whose `[scripts]` lists `entries` (files placed by `setup` first).
    fn new(
        threads: usize,
        setup: impl FnOnce(&Path) -> Vec<ScriptEntry>,
        link: impl FnOnce(ScriptsModule) -> ScriptsModule,
        ring: Option<Arc<LogRing>>,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = ScriptsConfig::for_project(dir.path(), 11);
        config.modules = setup(dir.path());
        let module = link(ScriptsModule::new(config));
        let mut engine_config = EngineConfig::new(dir.path());
        engine_config.deterministic_ids = Some(7);
        engine_config.threads = NonZeroUsize::new(threads).unwrap();
        engine_config.journal.sync = false;
        engine_config.txn_timeout = Duration::from_secs(120);
        engine_config.sim_seed = 11;
        engine_config.log_ring = ring;
        engine_config.modules = vec![Arc::new(module)];
        let mut engine = Engine::new(engine_config).unwrap();
        let session = engine.open_session();
        let mut project = Self {
            dir,
            engine,
            session,
        };
        project.ok(
            "session.open",
            json!({"client": "test", "actor": "agent:test"}),
        );
        project.ok(
            "scene.new",
            json!({"path": "scenes/test.scene.ron", "name": "Test"}),
        );
        project
    }

    /// A project running the conformance module from `rel`, a copy of fixture `fixture`.
    fn conformance(
        kind: ScriptKind,
        rel: &str,
        fixture: PathBuf,
        ring: Option<Arc<LogRing>>,
    ) -> Self {
        let rel = rel.to_owned();
        Self::new(
            1,
            move |dir| {
                place(dir, &rel, &fixture);
                vec![entry(dir, kind, &rel)]
            },
            |m| m,
            ring,
        )
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

    fn get(&mut self, world: &str, id: &str, component: &str) -> Value {
        self.ok(
            "component.get",
            json!({"entity": id, "component": component, "world": world}),
        )["value"]
            .clone()
    }

    fn step(&mut self, ticks: u32) -> String {
        self.ok("sim.step", json!({"ticks": ticks}))["world_hash"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn play_hash(&self) -> String {
        let world = self.engine.world(WorldSel::play).unwrap();
        world.state_hash().unwrap().to_string()
    }
}

fn mover(p: &mut Project) -> String {
    p.spawn(json!({
        "conf.Position": {"x": 1.0, "y": 2.0},
        "conf.Velocity": {"x": 1.0, "y": 3.0},
    }))
}

fn f32_of(value: &Value, field: &str) -> f32 {
    value[field]
        .as_f64()
        .unwrap_or_else(|| panic!("{field} in {value}")) as f32
}

const DT: f32 = (1.0f64 / 60.0) as f32;

#[test]
fn scripts_module_registers_module_types_and_adds_script_reload() {
    let mut p = Project::conformance(
        ScriptKind::Wasm,
        "scripts/conformance.wasm",
        wasm_fixture("conformance_guest"),
        None,
    );
    let core: Vec<&str> = concordia_api::methods().names().collect();
    let all: Vec<&str> = p.engine.methods().names().collect();
    let added = [
        "script.reload",
        "graph.list_nodes",
        "graph.check",
        "graph.compile",
        "graph.apply",
        "graph.trace",
    ];
    assert_eq!(
        all.len(),
        core.len() + added.len(),
        "the module adds script.reload and the graph methods"
    );
    assert!(added.iter().all(|m| all.contains(m)), "{all:?}");
    let schema = p.ok("schema.get", json!({"name": "conf.Position"}));
    assert_eq!(schema["schema"]["type"], "object", "{schema}");
    let id = mover(&mut p);
    assert_eq!(
        p.get("edit", &id, "conf.Position"),
        json!({"x": 1.0, "y": 2.0})
    );
    p.step(1);
    let moved = p.get("play", &id, "conf.Position");
    assert_eq!(
        f32_of(&moved, "x"),
        1.0 + DT,
        "the module's processors run in play"
    );
}

/// Warnings that a script processor faulted.
fn faults_logged(p: &mut Project) -> usize {
    let records = p.ok("log.tail", json!({"level": "warn"}))["records"].clone();
    records
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["message"] == "script processor faulted and was disabled")
        .count()
}

#[test]
fn a_new_play_session_reenables_faulted_processors() {
    let ring = Arc::new(LogRing::new(NonZeroUsize::new(256).unwrap()));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let fixture = wasm_fixture("conformance_guest");
        let mut p = Project::conformance(
            ScriptKind::Wasm,
            "scripts/conformance.wasm",
            fixture,
            Some(Arc::clone(&ring)),
        );
        p.spawn(json!({"conf.Faulty": null}));
        let id = mover(&mut p);
        p.step(3);
        assert_eq!(faults_logged(&mut p), 1, "faulted once, then disabled");
        p.ok("sim.stop", json!({}));
        p.step(1);
        assert_eq!(faults_logged(&mut p), 2, "a new session runs it again");
        assert_eq!(
            f32_of(&p.get("play", &id, "conf.Position"), "x"),
            1.0 + DT,
            "fresh play world"
        );
    });
}

#[test]
fn modules_that_fail_to_load_are_skipped_and_reported() {
    let mut p = Project::new(
        1,
        |dir| vec![entry(dir, ScriptKind::Wasm, "scripts/missing.wasm")],
        |m| m,
        None,
    );
    assert!(
        p.call("schema.get", json!({"name": "conf.Position"}))
            .is_err(),
        "nothing loaded"
    );
    let err = p.call("script.reload", json!({})).unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::ScriptFailed, "{err:?}");
    assert!(err.message.contains("missing.wasm"), "{}", err.message);
    let err = p
        .call("script.reload", json!({"module": "nope"}))
        .unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::ScriptFailed);
    assert!(err.message.contains("nope"), "{}", err.message);
}

#[test]
fn native_reload_refuses_a_module_whose_source_keeps_hidden_state() {
    let guest_src =
        concordia_tests::repo_root().join("tests/fixtures/script/conformance-guest/src");
    let mut p = Project::new(
        1,
        |dir| {
            let rel = format!("scripts/conformance.{}", std::env::consts::DLL_EXTENSION);
            place(dir, &rel, &native_fixture("conformance_guest"));
            vec![ScriptEntry {
                kind: ScriptKind::Native,
                path: dir.join("scripts/conformance"),
                source: Some(guest_src),
            }]
        },
        |m| m,
        None,
    );
    let err = p.call("script.reload", json!({})).unwrap_err();
    assert!(err.message.contains("static RUNS"), "{}", err.message);
}

#[test]
fn script_reload_over_api_is_logged_with_its_actor() {
    let ring = Arc::new(LogRing::new(NonZeroUsize::new(256).unwrap()));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let fixture = wasm_fixture("conformance_guest");
        let mut p = Project::new(
            1,
            move |dir| {
                place(dir, "scripts/conformance.wasm", &fixture);
                vec![entry(dir, ScriptKind::Wasm, "scripts/conformance.wasm")]
            },
            |m| m,
            Some(Arc::clone(&ring)),
        );
        let result = p.ok("script.reload", json!({"actor": "agent:gate"}));
        assert_eq!(result["modules"][0]["name"], "conformance");
        assert_eq!(result["modules"][0]["backend"], "wasm");
        assert_eq!(result["modules"][0]["migrated"], json!([]));
        let records = p.ok("log.tail", json!({"level": "info"}))["records"].clone();
        let reloaded = records
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["message"] == "script reloaded")
            .unwrap_or_else(|| panic!("no 'script reloaded' record: {records}"));
        assert_eq!(reloaded["fields"]["actor"], "agent:gate");
        assert_eq!(reloaded["fields"]["module"], "conformance");
        let history = p.ok("txn.history", json!({}));
        assert_eq!(
            history["entries"],
            json!([]),
            "nothing changed, nothing journaled"
        );
    });
}

#[test]
fn script_reload_journals_edit_world_migration_with_script_actor() {
    let library = format!("scripts/conformance.{}", std::env::consts::DLL_EXTENSION);
    let mut p = Project::conformance(
        ScriptKind::Native,
        &library,
        native_fixture("conformance_guest"),
        None,
    );
    let id = mover(&mut p);
    place(
        p.dir.path(),
        &library,
        &native_fixture("conformance_guest_layout2"),
    );
    let result = p.ok("script.reload", json!({}));
    let module = &result["modules"][0];
    assert!(module["edit_txn"].is_string(), "{result}");
    let migrated = module["migrated"].as_array().unwrap();
    let names: Vec<&str> = migrated
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["conf.Position", "conf.Velocity"], "{result}");
    assert_eq!(
        p.get("edit", &id, "conf.Position"),
        json!({"x": 1.0, "y": 2.0, "z": 0.0})
    );
    assert_eq!(p.get("edit", &id, "conf.Velocity"), json!({"x": 1.0}));

    let history = p.ok("txn.history", json!({"limit": 1}));
    let entry = &history["entries"][0];
    assert_eq!(entry["actor"], "script:conformance", "{history}");
    assert!(
        entry["label"].as_str().unwrap().contains("migrate"),
        "{history}"
    );
    assert_eq!(entry["txn"], module["edit_txn"]);

    p.ok("txn.undo", json!({}));
    assert_eq!(
        p.get("edit", &id, "conf.Position"),
        json!({"x": 1.0, "y": 2.0}),
        "exact old values"
    );
    assert_eq!(
        p.get("edit", &id, "conf.Velocity"),
        json!({"x": 1.0, "y": 3.0})
    );
    p.ok("txn.redo", json!({}));
    assert_eq!(p.get("edit", &id, "conf.Velocity"), json!({"x": 1.0}));
    p.ok("scene.save", json!({}));
}

#[test]
fn editing_a_native_module_during_a_headless_session_changes_behavior_after_reload() {
    let library = format!("scripts/conformance.{}", std::env::consts::DLL_EXTENSION);
    let mut p = Project::conformance(
        ScriptKind::Native,
        &library,
        native_fixture("conformance_guest"),
        None,
    );
    let id = mover(&mut p);
    p.step(2);
    let x = |p: &mut Project| f32_of(&p.get("play", &id, "conf.Position"), "x");
    let before = (p.play_hash(), x(&mut p));
    // The developer edits the module and rebuilds it in place (here: the v2 build, which moves
    // at double speed); script.reload picks up what is on disk.
    place(
        p.dir.path(),
        &library,
        &native_fixture("conformance_guest_v2"),
    );
    let result = p.ok("script.reload", json!({"module": "conformance"}));
    assert_eq!(result["modules"][0]["version"], 2);
    assert_eq!(
        (p.play_hash(), x(&mut p)),
        before,
        "the world continues from the same state"
    );
    p.step(1);
    assert_eq!(x(&mut p), before.1 + 1.0 * (DT * 2.0), "the new code runs");
}

/// The wanderers sample, 300 of them, on `threads` threads, stepped 150 ticks; hashes at
/// ticks 1, 60 and 150, and how many times one tired.
fn wanderers(threads: usize, how: &'static str) -> (Vec<String>, usize) {
    let ring = Arc::new(LogRing::new(NonZeroUsize::new(100_000).unwrap()));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let mut p = Project::new(
            threads,
            |dir| match how {
                "wasm" => {
                    place(dir, "scripts/wanderers.wasm", &wasm_fixture("wanderers"));
                    vec![entry(dir, ScriptKind::Wasm, "scripts/wanderers.wasm")]
                }
                "native" => {
                    let rel = format!("scripts/wanderers.{}", std::env::consts::DLL_EXTENSION);
                    place(dir, &rel, &native_fixture("wanderers"));
                    vec![entry(dir, ScriptKind::Native, "scripts/wanderers")]
                }
                _ => Vec::new(),
            },
            |m| match how {
                "linked" => m.link::<wanderers::Wanderers>(),
                _ => m,
            },
            Some(Arc::clone(&ring)),
        );
        for i in 0..300 {
            p.spawn(json!({
                "wanderers.Position": {"x": (i % 20) as f32, "y": (i / 20) as f32},
                "wanderers.Wander": {"heading": i as f32 * 0.1, "speed": 1.5, "turn": 2.0},
                "wanderers.Energy": {"value": (i % 10) as f32 / 10.0},
            }));
        }
        let hashes = vec![p.step(1), p.step(59), p.step(90)];
        let records = p.ok("log.tail", json!({"level": "info", "limit": 10000}))["records"].clone();
        let tired = records
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                r["message"]
                    .as_str()
                    .is_some_and(|m| m.starts_with("tired at"))
            })
            .count();
        (hashes, tired)
    })
}

#[test]
fn every_backend_runs_the_sample_logic_to_identical_hashes() {
    let (reference, tired) = wanderers(1, "linked");
    assert!(tired > 0, "the sample exercised signals and commands");
    for how in ["linked", "wasm", "native"] {
        for threads in [1, 8] {
            assert_eq!(
                wanderers(threads, how),
                (reference.clone(), tired),
                "{how} at {threads} threads"
            );
        }
    }
}

#[test]
fn scripts_fingerprint_each_loaded_file_by_its_content() {
    let library = format!("scripts/conformance.{}", std::env::consts::DLL_EXTENSION);
    let mut p = Project::conformance(
        ScriptKind::Native,
        &library,
        native_fixture("conformance_guest"),
        None,
    );
    let prints = p.engine.fingerprints();
    assert_eq!(prints.len(), 1, "{prints:?}");
    let print = &prints[0];
    assert_eq!(
        (
            print.module.as_str(),
            print.name.as_str(),
            print.kind.as_str()
        ),
        ("scripts", "conformance", "native")
    );
    let bytes = std::fs::read(p.dir.path().join(&library)).unwrap();
    assert_eq!(print.hash, blake3::hash(&bytes).to_hex().to_string());

    p.ok("script.reload", json!({}));
    assert_eq!(
        p.engine.fingerprints(),
        prints,
        "an identical reload keeps it"
    );
    place(
        p.dir.path(),
        &library,
        &native_fixture("conformance_guest_v2"),
    );
    p.ok("script.reload", json!({"module": "conformance"}));
    assert_ne!(
        p.engine.fingerprints()[0].hash,
        print.hash,
        "a new build changes it"
    );
}

#[test]
fn linked_modules_are_fingerprinted_by_their_description() {
    use concordia_script_guest::Module;
    let p = Project::new(
        1,
        |_| Vec::new(),
        |m| m.link::<wanderers::Wanderers>(),
        None,
    );
    let prints = p.engine.fingerprints();
    assert_eq!(prints.len(), 1, "{prints:?}");
    let desc = serde_json::to_vec(&wanderers::Wanderers::describe()).unwrap();
    assert_eq!(
        (prints[0].name.as_str(), prints[0].kind.as_str()),
        ("wanderers", "linked")
    );
    assert_eq!(prints[0].hash, blake3::hash(&desc).to_hex().to_string());
}

/// A graph module (M4 4.10b): counts ticks in a variable and moves up by dt (× `bonus` in v2).
fn mover_graph(version: u32) -> String {
    use concordia_graph::{ExecLink, Graph, GraphKind, Input, Literal, Node, Signature, Variable};
    let id = |n: u32| format!("0193b0f2-0000-7000-8000-{n:012x}");
    let link = |n: u32, pin: &str| Input::Link(id(n), pin.into());
    let float = |v: f64| Input::Value(Literal::Float(v));
    let mut variables = vec![Variable {
        name: "ticks".into(),
        ty: "i32".into(),
        default: Some(Literal::Int(0)),
    }];
    let mut speed = link(1, "dt");
    let mut nodes = vec![
        (1, Node::new("event.on_tick")),
        (2, Node::new("sequence")),
        (3, Node::new("var.get.ticks")),
        (
            4,
            Node::new("math.add")
                .with_input("a", link(3, "value"))
                .with_input("b", Input::Value(Literal::Int(1))),
        ),
        (
            5,
            Node::new("var.set.ticks").with_input("value", link(4, "out")),
        ),
        (6, Node::new("get.Transform.translation")),
    ];
    if version >= 2 {
        variables.push(Variable {
            name: "bonus".into(),
            ty: "f32".into(),
            default: Some(Literal::Float(2.0)),
        });
        nodes.push((9, Node::new("var.get.bonus")));
        nodes.push((
            10,
            Node::new("math.mul")
                .with_input("a", link(1, "dt"))
                .with_input("b", link(9, "value")),
        ));
        speed = link(10, "out");
    }
    nodes.push((
        7,
        Node::new("conv.vec3")
            .with_input("x", float(0.0))
            .with_input("y", speed)
            .with_input("z", float(0.0)),
    ));
    let add = if version == 3 {
        "math.addd"
    } else {
        "math.add"
    };
    nodes.push((
        8,
        Node::new(add)
            .with_input("a", link(6, "value"))
            .with_input("b", link(7, "out")),
    ));
    nodes.push((
        11,
        Node::new("set.Transform.translation").with_input("value", link(8, "out")),
    ));
    let graph = Graph {
        id: id(100),
        kind: GraphKind::Event,
        version: 1,
        signature: Signature::default(),
        variables,
        nodes: nodes.into_iter().map(|(n, node)| (id(n), node)).collect(),
        exec: vec![
            ExecLink(id(1), "out".into(), id(2), "in".into()),
            ExecLink(id(2), "then0".into(), id(5), "in".into()),
            ExecLink(id(2), "then1".into(), id(11), "in".into()),
        ],
        layout: std::collections::BTreeMap::new(),
    };
    concordia_graph::write_graph(&graph).unwrap()
}

fn graph_project() -> Project {
    Project::new(
        1,
        |dir| {
            std::fs::create_dir_all(dir.join("graphs")).unwrap();
            std::fs::write(dir.join("graphs/mover.graph.ron"), mover_graph(1)).unwrap();
            vec![entry(dir, ScriptKind::Graph, "graphs/mover.graph.ron")]
        },
        |m| m,
        None,
    )
}

fn y(p: &mut Project, id: &str) -> f64 {
    p.get("play", id, "Transform")["translation"][1]
        .as_f64()
        .unwrap()
}

#[test]
fn graph_modules_load_and_run_in_play() {
    let mut p = graph_project();
    let moved = p.spawn(json!({"Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]}, "graph.mover.Vars": {}}));
    let still = p.spawn(json!({"Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]}}));
    p.step(60);
    assert!(
        (y(&mut p, &moved) - 1.0).abs() < 1e-4,
        "{}",
        y(&mut p, &moved)
    );
    assert_eq!(y(&mut p, &still), 0.0);
    assert_eq!(p.get("play", &moved, "graph.mover.Vars")["ticks"], 60);
}

#[test]
fn hot_reload_preserves_variables_with_same_name_and_type() {
    let mut p = graph_project();
    let e = p.spawn(json!({"Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]}, "graph.mover.Vars": {}}));
    p.step(30);
    std::fs::write(p.dir.path().join("graphs/mover.graph.ron"), mover_graph(2)).unwrap();
    let result = p.ok("script.reload", json!({}));
    assert_eq!(result["modules"][0]["backend"], "graph");
    p.step(30);
    let vars = p.get("play", &e, "graph.mover.Vars");
    assert_eq!(vars["ticks"], 60, "kept across the reload: {vars}");
    assert_eq!(vars["bonus"], 2.0, "a new variable starts at its default");
    // 30 ticks at 1×, then 30 at 2×.
    assert!((y(&mut p, &e) - 1.5).abs() < 1e-4, "{}", y(&mut p, &e));
}

#[test]
fn hot_reload_of_broken_graph_keeps_previous_version_and_reports_diagnostics() {
    let mut p = graph_project();
    let e = p.spawn(json!({"Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]}, "graph.mover.Vars": {}}));
    p.step(30);
    std::fs::write(p.dir.path().join("graphs/mover.graph.ron"), mover_graph(3)).unwrap();
    let err = p.call("script.reload", json!({})).unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::ScriptFailed);
    assert!(
        err.message.contains("unknown_op") && err.message.contains("math.add"),
        "{}",
        err.message
    );
    p.step(30);
    assert_eq!(
        p.get("play", &e, "graph.mover.Vars")["ticks"],
        60,
        "the previous version keeps running"
    );
}

/// The edits for "on tick, move up by dt" (what an agent sends `graph.apply`).
fn rise_edits() -> Value {
    let id = |n: u32| format!("0193b0f2-0000-7000-8000-{n:012x}");
    json!([
        {"AddNode": {"id": id(1), "op": "event.on_tick", "at": [0.0, 0.0]}},
        {"AddNode": {"id": id(2), "op": "get.Transform.translation", "at": null}},
        {"AddNode": {"id": id(3), "op": "conv.vec3", "at": null}},
        {"AddNode": {"id": id(4), "op": "math.add", "at": null}},
        {"AddNode": {"id": id(5), "op": "set.Transform.translation", "at": null}},
        {"SetValue": {"node": id(3), "pin": "x", "value": {"Float": 0.0}}},
        {"Connect": {"from": id(1), "from_pin": "dt", "to": id(3), "to_pin": "y"}},
        {"SetValue": {"node": id(3), "pin": "z", "value": {"Float": 0.0}}},
        {"Connect": {"from": id(2), "from_pin": "value", "to": id(4), "to_pin": "a"}},
        {"Connect": {"from": id(3), "from_pin": "out", "to": id(4), "to_pin": "b"}},
        {"Connect": {"from": id(4), "from_pin": "out", "to": id(5), "to_pin": "value"}},
        {"ConnectExec": {"from": id(1), "from_pin": "out", "to": id(5), "to_pin": "in"}},
    ])
}

/// A project listing `graphs/rise.graph.ron`, which doesn't exist yet.
fn empty_graph_project() -> Project {
    Project::new(
        1,
        |dir| vec![entry(dir, ScriptKind::Graph, "graphs/rise.graph.ron")],
        |m| m,
        None,
    )
}

#[test]
fn graph_methods_list_check_compile_and_apply() {
    let mut p = empty_graph_project();
    let nodes = p.ok("graph.list_nodes", json!({}))["nodes"].clone();
    assert!(
        nodes.get("event.on_tick").is_some() && nodes.get("get.Transform.translation").is_some()
    );
    // An agent builds the graph in two steps: a broken half (written, with its diagnostics)...
    let edits = rise_edits();
    let (first, rest) = edits.as_array().unwrap().split_at(5);
    let half = p.ok(
        "graph.apply",
        json!({"path": "graphs/rise.graph.ron", "kind": "Event", "edits": first}),
    );
    assert_eq!(half["ok"], false);
    assert!(
        half["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "unconnected_input"),
        "{half}"
    );
    assert_eq!(half["reloaded"], false, "a broken graph isn't reloaded");
    let half_text = std::fs::read_to_string(p.dir.path().join("graphs/rise.graph.ron")).unwrap();
    // ...then the rest, which checks clean, so the module joins.
    let done = p.ok(
        "graph.apply",
        json!({"path": "graphs/rise.graph.ron", "edits": rest}),
    );
    assert_eq!(done["ok"], true, "{done}");
    assert_eq!(done["reloaded"], true, "{done}");
    let checked = p.ok("graph.check", json!({"path": "graphs/rise.graph.ron"}));
    assert_eq!(checked["ok"], true);
    let compiled = p.ok("graph.compile", json!({"path": "graphs/rise.graph.ron"}));
    assert_eq!(compiled["processors"][0]["name"], "graph.rise.tick");
    assert_eq!(compiled["processors"][0]["writes"], json!(["Transform"]));
    assert!(compiled["processors"][0]["wasm_bytes"].as_u64().unwrap() > 0);
    // Text can be checked without a file too.
    let text = std::fs::read_to_string(p.dir.path().join("graphs/rise.graph.ron")).unwrap();
    let broken = text.replace("math.add", "math.addd");
    let checked = p.ok("graph.check", json!({"text": broken}));
    assert_eq!(checked["ok"], false);
    assert_eq!(checked["diagnostics"][0]["code"], "unknown_op");
    // Each apply is one undoable transaction by the session's actor; undo restores the file.
    let history = p.ok("txn.history", json!({}))["entries"].clone();
    let last = history.as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["actor"], "agent:test", "{last}");
    p.ok("txn.undo", json!({}));
    let after_undo = std::fs::read_to_string(p.dir.path().join("graphs/rise.graph.ron")).unwrap();
    assert_eq!(
        after_undo, half_text,
        "back to the first half, byte for byte"
    );
    p.ok("txn.undo", json!({}));
    assert!(
        !p.dir.path().join("graphs/rise.graph.ron").exists(),
        "back to no file"
    );
}

#[test]
fn graph_paths_must_be_graph_files_in_the_project() {
    let mut p = empty_graph_project();
    for bad in ["../x.graph.ron", "graphs/x.ron", "/tmp/x.graph.ron"] {
        let err = p
            .call("graph.apply", json!({"path": bad, "edits": []}))
            .unwrap_err();
        assert_eq!(err.kind, ApiErrorKind::InvalidParams, "{bad}");
    }
}

#[test]
fn mcp_graph_tools_schemas_generated_from_reflect() {
    let module = ScriptsModule::new(ScriptsConfig::for_project(Path::new("."), 1));
    let schemas: Vec<Value> = concordia_api::EngineModule::methods(&module)
        .iter()
        .filter(|m| m.name.starts_with("graph."))
        .map(|m| json!({"name": m.name, "mutates": m.mutates, "docs": m.docs, "params": concordia_api::params_schema(m)}))
        .collect();
    assert_eq!(schemas.len(), 5, "{schemas:?}");
    insta::assert_json_snapshot!(schemas);
}
