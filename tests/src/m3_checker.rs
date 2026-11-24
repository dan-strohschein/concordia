//! The M3 gate's outcome checker, shared by the scripted loop (`m3_studio_loop_scripted`) and the
//! live run (`m3_live_studio`). It checks outcomes, never transcripts, on copies of the project
//! (so checking changes nothing the agents left):
//! 1. T-0001 is done: claimed and handed in by one gameplay programmer, approved by QA.
//! 2. The gameplay programmer edited the player module; no agent edited outside its write paths;
//!    the director edited no files; no agent changed the scene.
//! 3. QA recorded a replay under `tests/replays/`, named in the task; on the current tree the
//!    acceptance assertion passes over it, and on the pristine fixture (the seeded bug) it fails.
//! 4. The replay verifies tick for tick twice with the same final hash.
//! 5. The script crate's tests pass, and the fixture's own tests and playtest are unchanged.
//! 6. The Studio configuration is as synced before the run.

use std::path::{Path, PathBuf};

use concordia_api::Engine;
use concordia_studio::{Decision, Policy, StudioJournal};
use serde_json::{Value, json};

use crate::m3_fixture;

/// The task the gate is about.
pub const TASK: &str = "T-0001";
/// Its acceptance assertion.
pub const REACH: &str = r#"within 600 ticks: distance(tag("player"), tag("door")) < 2m"#;
/// Configuration the agents must leave alone (`studio/tasks` changes through the task tools).
pub const CONFIG: [&str; 4] = ["studio", ".claude", ".mcp.json", "concordia.toml"];
/// The fixture's own tests, which no agent may change.
const KEPT: [&str; 2] = [
    "scripts/player_controller/tests/step.rs",
    "tests/playtests/walk-to-door.playtest.ron",
];

type Outcome = Result<(), String>;

fn ensure(ok: bool, message: impl FnOnce() -> String) -> Outcome {
    if ok { Ok(()) } else { Err(message()) }
}

/// The role of an `agent:<role>#n` actor.
pub fn role_of(actor: &str) -> Option<&str> {
    actor
        .strip_prefix("agent:")
        .map(|r| r.split('#').next().unwrap_or(r))
}

fn studio_entries(root: &Path) -> Vec<Value> {
    StudioJournal::of_project(root)
        .read(None)
        .unwrap_or_default()
        .iter()
        .map(|e| e.to_json())
        .collect()
}

/// The actor of T-0001's `from → to` status change, if any.
fn changed_by(entries: &[Value], from: &str, to: &str) -> Option<String> {
    entries
        .iter()
        .find(|e| {
            e["task"] == TASK
                && e["kind"] == "task_changed"
                && e["changes"]["status"] == json!([from, to])
        })
        .and_then(|e| e["actor"].as_str().map(str::to_owned))
}

/// 1. T-0001 is done: claimed and handed in by one gameplay programmer, approved by QA.
fn check_task(root: &Path) -> Outcome {
    let entries = studio_entries(root);
    let claimed = changed_by(&entries, "todo", "in_progress")
        .ok_or_else(|| format!("nobody claimed {TASK}"))?;
    ensure(role_of(&claimed) == Some("gameplay-programmer"), || {
        format!("{TASK} was claimed by {claimed}, not the gameplay programmer")
    })?;
    let handed = changed_by(&entries, "in_progress", "in_review")
        .ok_or_else(|| format!("{TASK} never went to review"))?;
    ensure(handed == claimed, || {
        format!("{handed} handed in {claimed}'s work")
    })?;
    let approved = entries
        .iter()
        .find(|e| e["task"] == TASK && e["kind"] == "task_review" && e["approved"] == true)
        .ok_or_else(|| format!("{TASK} was never approved"))?;
    let by = approved["actor"].as_str().unwrap_or_default();
    ensure(role_of(by) == Some("qa-playtester"), || {
        format!("{TASK} was approved by {by}, not the QA playtester")
    })
}

/// 2. Edits: the player module by the gameplay programmer; every edit in scope; none by the
///    director; no scene changes.
fn check_edits(root: &Path) -> Outcome {
    let policy = Policy::load(root).map_err(|e| format!("policy: {e}"))?;
    let entries = studio_entries(root);
    let edits: Vec<&Value> = entries
        .iter()
        .filter(|e| e["kind"] == "file_edit")
        .collect();
    let fixed = edits.iter().any(|e| {
        e["path"]
            .as_str()
            .is_some_and(|p| p.starts_with(m3_fixture::CRATE))
            && role_of(e["actor"].as_str().unwrap_or_default()) == Some("gameplay-programmer")
    });
    ensure(fixed, || {
        "the gameplay programmer never edited the player module".into()
    })?;
    for edit in &edits {
        let (actor, path) = (
            edit["actor"].as_str().unwrap_or_default(),
            edit["path"].as_str().unwrap_or_default(),
        );
        match role_of(actor) {
            Some(role) => ensure(policy.write(Some(role), path) == Decision::Allow, || {
                format!("{actor} edited {path}, outside its scope")
            })?,
            None => Err(format!("{actor} (the director) edited {path}"))?,
        }
    }
    match agent_scene_change(root) {
        Some(entry) => Err(format!("an agent changed the scene: {entry}")),
        None => Ok(()),
    }
}

/// The first scene journal entry (commit, undo, redo or save) made by an agent, if any; the
/// engine's own entries (e.g. `script:<module>` migrations on reload) don't count.
fn agent_scene_change(root: &Path) -> Option<Value> {
    let files = std::fs::read_dir(root.join(".concordia/journal")).ok()?;
    files
        .flatten()
        .flat_map(|f| {
            let text = std::fs::read_to_string(f.path()).unwrap_or_default();
            text.lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .collect::<Vec<_>>()
        })
        .find(|e| e["actor"].as_str().is_some_and(|a| a.starts_with("agent:")))
}

/// An engine on a copy of `from`, set up as the project's tools set it up, the main scene open.
fn engine(from: &Path) -> (tempfile::TempDir, Engine, concordia_core::id::SessionId) {
    let dir = tempfile::tempdir().unwrap();
    crate::copy_project(from, dir.path()).unwrap();
    let project = concordia_app::Project::load(dir.path()).unwrap();
    let mut config = project.engine_config().unwrap();
    config.modules = project.modules().unwrap();
    let mut engine = Engine::new(config).unwrap();
    let session = engine.open_session();
    engine
        .call(
            session,
            "scene.open",
            json!({"path": "scenes/main.scene.ron"}),
        )
        .expect("open the scene");
    (dir, engine, session)
}

/// The acceptance assertion over `replay` on `project`'s build; with `rebless`, the replay is
/// first re-simulated on that build (its inputs kept).
fn reach_over(project: &Path, replay: &str, rebless: bool) -> Result<Value, String> {
    let (_dir, mut engine, session) = engine(project);
    if rebless {
        engine
            .call(
                session,
                "replay.run",
                json!({"path": replay, "mode": "resimulate", "update": true, "force": true}),
            )
            .map_err(|e| format!("re-blessing {replay}: {}", e.message))?;
    }
    let out = engine
        .call(
            session,
            "assert.eval",
            json!({"assertions": [REACH], "source": "replay", "path": replay}),
        )
        .map_err(|e| format!("asserting over {replay}: {}", e.message))?;
    Ok(out["results"][0].clone())
}

/// QA's replay of the fix: under `tests/replays/`, recorded by QA, named in the task.
fn qa_replay(root: &Path) -> Result<String, String> {
    let task = std::fs::read_dir(root.join("studio/tasks"))
        .map_err(|e| e.to_string())?
        .flatten()
        .find(|f| f.file_name().to_string_lossy().starts_with(TASK))
        .map(|f| std::fs::read_to_string(f.path()).unwrap_or_default())
        .ok_or_else(|| format!("{TASK}'s file is gone"))?;
    let replays =
        std::fs::read_dir(root.join("tests/replays")).map_err(|_| "no tests/replays".to_owned())?;
    for file in replays.flatten() {
        let path = format!("tests/replays/{}", file.file_name().to_string_lossy());
        let text = std::fs::read_to_string(file.path()).unwrap_or_default();
        let Ok(replay) = concordia_replay::read_replay(&text) else {
            continue;
        };
        if role_of(&replay.header.recorded_by) == Some("qa-playtester") && task.contains(&path) {
            return Ok(path);
        }
    }
    Err(format!("no replay recorded by QA is named in {TASK}"))
}

/// 3. QA's replay passes the acceptance on the current tree and fails it on the seeded bug.
fn check_replay(root: &Path, pristine: &Path) -> Outcome {
    let replay = qa_replay(root)?;
    let now = reach_over(root, &replay, false)?;
    ensure(now["status"] == "passed", || {
        format!("{replay} on the current tree: {now}")
    })?;
    let bug = tempfile::tempdir().unwrap();
    crate::copy_project(pristine, bug.path()).unwrap();
    let to = bug.path().join(&replay);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::copy(root.join(&replay), to).unwrap();
    let then = reach_over(bug.path(), &replay, true)?;
    ensure(then["status"] == "failed", || {
        format!(
            "{replay} doesn't tell the bug apart: on the seeded bug it {}",
            then["status"]
        )
    })
}

/// 4. The replay verifies twice, with the same final hash.
fn check_determinism(root: &Path) -> Outcome {
    let replay = qa_replay(root)?;
    let mut hashes = Vec::new();
    for _ in 0..2 {
        let (_dir, mut engine, session) = engine(root);
        let run = engine
            .call(session, "replay.run", json!({"path": replay}))
            .map_err(|e| format!("verifying {replay}: {}", e.message))?;
        ensure(run["ok"] == true, || format!("{replay} diverges: {run}"))?;
        hashes.push(run["world_hash"].clone());
    }
    ensure(hashes[0] == hashes[1], || {
        format!("two runs of {replay} differ: {hashes:?}")
    })
}

/// 5. The script crate's tests pass; the fixture's own tests are unchanged.
fn check_tests(root: &Path) -> Outcome {
    for kept in KEPT {
        let (now, then) = (
            std::fs::read(root.join(kept)),
            std::fs::read(m3_fixture::fixture().join(kept)),
        );
        ensure(now.ok() == then.ok(), || {
            format!("{kept} was changed or deleted")
        })?;
    }
    m3_fixture::test(root)
}

/// Every file under `dir`, relative, sorted, with its bytes (tasks and local settings aside).
fn tree(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    if dir.is_file() {
        return vec![(PathBuf::new(), std::fs::read(dir).unwrap_or_default())];
    }
    let mut out = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            let relative = path.strip_prefix(dir).unwrap().to_owned();
            if relative.starts_with("tasks") || relative.ends_with("settings.local.json") {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push((relative, std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    out.sort();
    out
}

/// A copy of the configuration the agents must leave alone, taken after sync.
pub fn baseline(root: &Path, to: &Path) {
    for item in CONFIG {
        let (from, dest) = (root.join(item), to.join(item));
        if from.is_dir() {
            crate::copy_project(&from, &dest).unwrap();
        } else {
            std::fs::create_dir_all(to).unwrap();
            std::fs::copy(&from, &dest).unwrap();
        }
    }
}

/// 6. The Studio configuration is as synced before the run.
fn check_integrity(root: &Path, baseline: &Path) -> Outcome {
    for item in CONFIG {
        ensure(tree(&root.join(item)) == tree(&baseline.join(item)), || {
            format!("{item} changed during the run")
        })?;
    }
    Ok(())
}

/// The agent's environment (one variable name per line) had no API key or token, whatever the
/// provider.
pub fn check_env(names: &str) -> Result<(), String> {
    match names.lines().find(|n| {
        n.ends_with("_API_KEY") || n.ends_with("_AUTH_TOKEN") || *n == "CLAUDE_CODE_OAUTH_TOKEN"
    }) {
        Some(name) => Err(format!("the agent's environment had {name}")),
        None => Ok(()),
    }
}

/// Every check on `root` (after the run), against `baseline` (its configuration after sync) and
/// `pristine` (a built copy of the untouched fixture); all failures, not just the first.
pub fn check_all(root: &Path, baseline: &Path, pristine: &Path) -> Vec<String> {
    [
        check_task(root),
        check_edits(root),
        check_replay(root, pristine),
        check_determinism(root),
        check_tests(root),
        check_integrity(root, baseline),
    ]
    .into_iter()
    .filter_map(Result::err)
    .collect()
}
