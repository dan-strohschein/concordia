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
    assert_eq!(all.len(), core.len() + 1, "the module adds one method");
    assert!(all.contains(&"script.reload"));
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
