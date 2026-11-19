//! M1 live gate (ADR-0049): outcome checks for a real Claude Code session that built a physics
//! scene from nothing over MCP (run via `just gate-live M1`). Asserts outcomes, never transcripts.
//!
//! Reads the agent's project (`M1_LIVE_PROJECT`), the `claude -p --output-format json` result
//! (`M1_LIVE_RESULT`), the MCP server's log (`M1_LIVE_LOG`) and the names of the variables in the
//! agent's environment (`M1_LIVE_ENV`).

use std::path::PathBuf;

use concordia_api::{Engine, EngineConfig};
use concordia_core::id::SessionId;
use serde_json::{Value, json};

const SCENE: &str = "scenes/agent.scene.ron";

fn env_path(var: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {var} (run via `just gate-live M1`)"))
}

fn read(var: &str) -> String {
    let path = env_path(var);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// An engine on a copy of the agent's project (so checking changes nothing it left behind).
struct Check {
    _dir: tempfile::TempDir,
    engine: Engine,
    session: SessionId,
}

impl Check {
    fn open() -> Self {
        let dir = tempfile::tempdir().unwrap();
        concordia_tests::copy_project(&env_path("M1_LIVE_PROJECT"), dir.path()).unwrap();
        let mut config = EngineConfig::new(dir.path());
        config.modules = concordia_app::Project::load(dir.path())
            .and_then(|p| p.modules())
            .unwrap();
        let mut engine = Engine::new(config).unwrap();
        let session = engine.open_session();
        let mut check = Self {
            _dir: dir,
            engine,
            session,
        };
        check.ok(
            "session.open",
            json!({"client": "m1-live-check", "actor": "tool:m1-live-check"}),
        );
        check.ok("scene.open", json!({"path": SCENE}));
        check
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        self.engine
            .call(self.session, method, params)
            .unwrap_or_else(|e| panic!("{method}: {e:?}"))
    }

    fn rows(&mut self, with: &[&str], world: &str) -> Vec<Value> {
        let result = self.ok(
            "query.run",
            json!({"with": with, "select": with, "world": world, "limit": 1000}),
        );
        result["rows"].as_array().unwrap().clone()
    }
}

/// Dynamic bodies by id, with their height.
fn dynamic_heights(check: &mut Check, world: &str) -> Vec<(Value, f64)> {
    check
        .rows(&["RigidBody", "Transform"], world)
        .into_iter()
        .filter(|r| r["components"]["RigidBody"]["kind"] == "Dynamic")
        .map(|r| {
            let y = r["components"]["Transform"]["translation"][1]
                .as_f64()
                .unwrap();
            (r["id"].clone(), y)
        })
        .collect()
}

#[test]
#[ignore = "run via just gate-live M1"]
fn m1_live_outcome_check() {
    let result: Value = serde_json::from_str(&read("M1_LIVE_RESULT")).unwrap();
    assert_eq!(result["is_error"], false, "the agent run failed: {result}");
    for name in read("M1_LIVE_ENV").lines() {
        assert!(
            !name.ends_with("_API_KEY") && !name.ends_with("_AUTH_TOKEN"),
            "the agent's environment had {name}"
        );
    }

    let mut check = Check::open();
    let files = check.ok("project.files", json!({}));
    let model = files["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "assets/crate.glb")
        .and_then(|f| f["asset"].as_str())
        .expect("the agent imported assets/crate.glb")
        .to_owned();
    let instanced = check.rows(&["MeshRenderer"], "edit").iter().any(|r| {
        r["components"]["MeshRenderer"]["mesh"]
            .as_str()
            .is_some_and(|m| m.starts_with(&format!("asset:{model}")))
    });
    assert!(
        instanced,
        "the scene holds an instance of the imported model"
    );
    assert!(
        !check.rows(&["DirectionalLight"], "edit").is_empty(),
        "a sun"
    );
    assert!(!check.rows(&["Camera"], "edit").is_empty(), "a camera");

    let start = dynamic_heights(&mut check, "edit");
    assert!(
        start.len() >= 5,
        "five or more dynamic boxes, found {}",
        start.len()
    );
    check.ok("sim.step", json!({"ticks": 120}));
    let end = dynamic_heights(&mut check, "play");
    let fell = start
        .iter()
        .filter(|(id, y)| end.iter().any(|(e, y2)| e == id && *y2 < y - 0.1))
        .count();
    assert!(
        fell >= 5,
        "the boxes fall in the play world ({fell} of {} fell)",
        start.len()
    );

    let info = check.ok("scene.info", json!({}));
    let journal = env_path("M1_LIVE_PROJECT")
        .join(".concordia/journal")
        .join(format!(
            "{}.cmdlog.jsonl",
            info["scene_id"].as_str().unwrap()
        ));
    let entries: Vec<Value> = std::fs::read_to_string(&journal)
        .unwrap_or_else(|e| panic!("{}: {e}", journal.display()))
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let edits: Vec<&Value> = entries
        .iter()
        .filter(|e| matches!(e["kind"].as_str(), Some("commit" | "undo" | "redo")))
        .collect();
    assert!(!edits.is_empty(), "the journal records the agent's edits");
    for entry in edits {
        let actor = entry["actor"].as_str().unwrap_or_default();
        assert!(actor.starts_with("agent:"), "edit by {actor}: {entry}");
    }

    let log = read("M1_LIVE_LOG");
    let by_agent = |message: &str| {
        log.lines()
            .any(|l| l.contains(message) && l.contains("actor=") && l.contains("agent:"))
    };
    assert!(
        by_agent("sim stepped"),
        "the log shows the agent stepping the simulation"
    );
    assert!(
        by_agent("captured screenshot"),
        "the log shows the agent capturing a screenshot"
    );
}
