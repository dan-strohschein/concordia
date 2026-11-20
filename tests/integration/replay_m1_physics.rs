//! M2 phase 2.8 done criterion: a recorded M1 physics-stack session replays bit-identically,
//! at any thread count, faster than real time.

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use concordia_api::{Engine, EngineConfig};
use concordia_core::id::SessionId;
use serde_json::{Value, json};

const REPLAY: &str = "tests/replays/stack.replay.ron";

/// An engine on `dir` (a copy of the example) with its modules plus replay, the stack scene open.
struct Stack {
    engine: Engine,
    session: SessionId,
}

impl Stack {
    fn new(dir: &Path, threads: usize) -> Self {
        let mut modules = concordia_app::Project::load(dir)
            .and_then(|p| p.modules())
            .expect("the example's modules load");
        modules.push(Arc::new(concordia_replay::ReplayModule::new()));
        let mut config = EngineConfig::new(dir);
        config.modules = modules;
        config.threads = NonZeroUsize::new(threads).unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut stack = Self { engine, session };
        stack.ok(
            "session.open",
            json!({"client": "replay-test", "actor": "agent:qa"}),
        );
        stack.ok("scene.open", json!({"path": "scenes/stack.scene.ron"}));
        stack
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {e:?}"))
    }

    /// Records 300 ticks (checkpoints every 60) and returns `sim.step`'s last hash.
    fn record(&mut self) -> String {
        self.ok(
            "replay.record",
            json!({"path": REPLAY, "checkpoint_every": 60}),
        );
        let hash = self.ok("sim.step", json!({"ticks": 300}))["world_hash"].clone();
        self.ok("replay.record", json!({"action": "stop"}));
        hash.as_str().unwrap().to_owned()
    }

    fn dropped_y(&mut self, world: &str) -> f64 {
        let rows = self.ok(
            "query.run",
            json!({"with": ["Name", "Transform"], "select": ["Name", "Transform"], "world": world}),
        );
        rows["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["components"]["Name"] == "Dropped 0")
            .map(|r| {
                r["components"]["Transform"]["translation"][1]
                    .as_f64()
                    .unwrap()
            })
            .expect("Dropped 0")
    }
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    concordia_tests::copy_example("m1-physics-stack", dir.path()).unwrap();
    dir
}

#[test]
fn recorded_m1_physics_session_replays_bit_identically() {
    let dir = project();
    let mut stack = Stack::new(dir.path(), 4);
    let start = stack.dropped_y("edit");
    let recorded = stack.record();
    assert!(stack.dropped_y("play") < start - 0.5, "physics ran");
    stack.ok("sim.stop", json!({}));

    let run = stack.ok("replay.run", json!({"path": REPLAY}));
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(
        run["matched"], 6,
        "a hash at ticks 0, 60, …, 300 each matched: {run}"
    );
    assert_eq!(
        run["world_hash"],
        recorded.as_str(),
        "bit-identical to the recorded run"
    );
}

#[test]
fn replay_is_identical_across_thread_counts() {
    let dir = project();
    let recorded = Stack::new(dir.path(), 1).record();
    for threads in [1, 8] {
        let run = Stack::new(dir.path(), threads).ok("replay.run", json!({"path": REPLAY}));
        assert_eq!(run["ok"], true, "{threads} threads: {run}");
        assert_eq!(run["world_hash"], recorded.as_str(), "{threads} threads");
    }
}

#[test]
fn replay_runs_headless_faster_than_realtime() {
    let dir = project();
    let mut stack = Stack::new(dir.path(), 4);
    stack.record();
    let run = stack.ok("replay.run", json!({"path": REPLAY}));
    assert_eq!(run["ticks"], 300);
    let (sim_ms, wall_ms) = (300.0 / 60.0 * 1000.0, run["elapsed_ms"].as_f64().unwrap());
    assert!(
        wall_ms < sim_ms,
        "replayed {sim_ms} ms of play in {wall_ms} ms"
    );
}
