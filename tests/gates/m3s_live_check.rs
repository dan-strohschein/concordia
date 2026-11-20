//! Studio checkpoint live gate (ADR-0049): outcome checks for a real `claude -p --agent director`
//! session that had the Level Designer build a lit physics playground and QA approve it (run via
//! `just gate-live M3S`). Asserts outcomes, never transcripts:
//! 1. a Level Designer task is done, approved by QA;
//! 2. the scene has a static ground, a light, a camera and at least three dynamic bodies;
//! 3. every scene edit is the Level Designer's;
//! 4. no agent's file edit is outside its role's write paths;
//! 5. no API key was in the agent's environment, and the Studio configuration is unchanged.
//!
//! The same checks run, non-ignored, against a scripted twin of the session (the real hook binary
//! and `concordia mcp`, no live agent), so the checker itself is tested without a subscription.

use std::path::{Path, PathBuf};

use concordia_api::{Engine, EngineConfig};
use concordia_studio::{Decision, Policy, StudioJournal};
use concordia_tests::mcp_client::McpClient;
use concordia_tests::studio_gate::{agent_call, mcp, new_project};
use serde_json::{Value, json};

const SCENE: &str = "scenes/playground.scene.ron";
/// Configuration the agents must leave alone (`studio/tasks` changes through the task tools).
const CONFIG: [&str; 4] = ["studio", ".claude", ".mcp.json", "concordia.toml"];

type Outcome = Result<(), String>;

fn ensure(ok: bool, message: impl FnOnce() -> String) -> Outcome {
    if ok { Ok(()) } else { Err(message()) }
}

/// The role of an `agent:<role>#n` actor.
fn role_of(actor: &str) -> Option<&str> {
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

/// 1. A Level Designer task is done, and QA approved it.
fn check_task(root: &Path) -> Outcome {
    let entries = studio_entries(root);
    let done: Vec<&Value> = entries
        .iter()
        .filter(|e| e["kind"] == "task_review" && e["approved"] == true)
        .collect();
    let task = done
        .iter()
        .find(|e| role_of(e["actor"].as_str().unwrap_or("")) == Some("qa-playtester"))
        .ok_or_else(|| format!("no task approved by the QA playtester: {done:?}"))?;
    let id = task["task"].as_str().unwrap_or_default();
    let created = entries
        .iter()
        .find(|e| e["kind"] == "task_created" && e["task"] == id)
        .ok_or_else(|| format!("{id} was approved but never created"))?;
    ensure(created["role"] == "level-designer", || {
        format!("{id} is for {}, not the level designer", created["role"])
    })?;
    let claimed = entries.iter().any(|e| {
        e["task"] == id
            && e["kind"] == "task_changed"
            && e["changes"]["status"] == json!(["todo", "in_progress"])
            && role_of(e["actor"].as_str().unwrap_or("")) == Some("level-designer")
    });
    ensure(claimed, || format!("the level designer never claimed {id}"))
}

/// An engine on a copy of the project (so checking changes nothing the agents left).
fn engine_on_copy(root: &Path) -> (tempfile::TempDir, Engine, concordia_core::id::SessionId) {
    let dir = tempfile::tempdir().unwrap();
    concordia_tests::copy_project(root, dir.path()).unwrap();
    let mut config = EngineConfig::new(dir.path());
    config.modules = concordia_app::Project::load(dir.path())
        .and_then(|p| p.modules())
        .unwrap();
    let mut engine = Engine::new(config).unwrap();
    let session = engine.open_session();
    engine
        .call(
            session,
            "session.open",
            json!({"client": "m3s-live-check", "actor": "tool:m3s-live-check"}),
        )
        .unwrap();
    (dir, engine, session)
}

/// 2. The scene has a static ground, a light, a camera and at least three dynamic bodies.
fn check_scene(root: &Path) -> Outcome {
    ensure(root.join(SCENE).is_file(), || {
        format!("{SCENE} was not saved")
    })?;
    let (_dir, mut engine, session) = engine_on_copy(root);
    engine
        .call(session, "scene.open", json!({"path": SCENE}))
        .map_err(|e| format!("{SCENE} does not open: {}", e.message))?;
    let mut rows = |with: &[&str]| -> Vec<Value> {
        let result = engine
            .call(
                session,
                "query.run",
                json!({"with": with, "select": with, "limit": 1000}),
            )
            .unwrap_or_else(|e| panic!("query {with:?}: {}", e.message));
        result["rows"].as_array().cloned().unwrap_or_default()
    };
    let bodies = |rows: &[Value], kind: &str| {
        rows.iter()
            .filter(|r| r["components"]["RigidBody"]["kind"] == kind)
            .count()
    };
    let solid = rows(&["RigidBody", "Collider"]);
    ensure(bodies(&solid, "Static") >= 1, || {
        "no static ground (RigidBody Static + Collider)".into()
    })?;
    ensure(bodies(&solid, "Dynamic") >= 3, || {
        format!(
            "{} dynamic bodies with colliders, want 3",
            bodies(&solid, "Dynamic")
        )
    })?;
    let lights: usize = ["DirectionalLight", "PointLight", "SpotLight"]
        .iter()
        .map(|l| rows(&[l]).len())
        .sum();
    ensure(lights >= 1, || "no light".into())?;
    ensure(!rows(&["Camera"]).is_empty(), || "no camera".into())
}

/// 3. Every scene edit (commit, undo, redo, save) is the Level Designer's.
fn check_scene_attribution(root: &Path) -> Outcome {
    let dir = root.join(".concordia/journal");
    let mut commits = 0;
    for file in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let text = std::fs::read_to_string(file.map_err(|e| e.to_string())?.path())
            .map_err(|e| e.to_string())?;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let entry: Value =
                serde_json::from_str(line).map_err(|e| format!("journal line: {e}"))?;
            let actor = entry["actor"].as_str().unwrap_or_default();
            ensure(role_of(actor) == Some("level-designer"), || {
                format!("a scene {} by {actor}", entry["kind"])
            })?;
            commits += usize::from(entry["kind"] == "commit");
        }
    }
    ensure(commits > 0, || "no scene edits at all".into())
}

/// 4. No agent's file edit is outside its role's write paths.
fn check_file_scope(root: &Path) -> Outcome {
    let policy = Policy::load(root).map_err(|e| format!("policy: {e}"))?;
    for edit in studio_entries(root)
        .iter()
        .filter(|e| e["kind"] == "file_edit")
    {
        let (actor, path) = (
            edit["actor"].as_str().unwrap_or_default(),
            edit["path"].as_str().unwrap_or_default(),
        );
        if let Some(role) = role_of(actor) {
            ensure(policy.write(Some(role), path) == Decision::Allow, || {
                format!("{actor} edited {path}, outside its scope")
            })?;
        }
    }
    Ok(())
}

/// Every file under `dir`, relative, sorted, with its bytes.
fn tree(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            if d.is_file() {
                out.push((PathBuf::new(), std::fs::read(&d).unwrap_or_default()));
            }
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let relative = path.strip_prefix(dir).unwrap().to_owned();
            let skip = relative.starts_with("tasks") || relative.ends_with("settings.local.json");
            if skip {
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

/// 5b. The Studio configuration is as synced before the run.
fn check_integrity(root: &Path, baseline: &Path) -> Outcome {
    for item in CONFIG {
        ensure(tree(&root.join(item)) == tree(&baseline.join(item)), || {
            format!("{item} changed during the run")
        })?;
    }
    Ok(())
}

/// 5a. The agent's environment had no API key or token.
fn check_env(names: &str) -> Outcome {
    match names
        .lines()
        .find(|n| n.ends_with("_API_KEY") || n.ends_with("_AUTH_TOKEN"))
    {
        Some(name) => Err(format!("the agent's environment had {name}")),
        None => Ok(()),
    }
}

/// Every check; all failures, not just the first.
fn check_all(root: &Path, baseline: &Path, env_names: &str) -> Vec<String> {
    [
        check_task(root),
        check_scene(root),
        check_scene_attribution(root),
        check_file_scope(root),
        check_env(env_names),
        check_integrity(root, baseline),
    ]
    .into_iter()
    .filter_map(Result::err)
    .collect()
}

fn env_path(var: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {var} (run via `just gate-live M3S`)"))
}

#[test]
#[ignore = "run via just gate-live M3S"]
fn m3s_live_outcome_check() {
    let result: Value =
        serde_json::from_str(&std::fs::read_to_string(env_path("M3S_LIVE_RESULT")).unwrap())
            .unwrap();
    assert_eq!(result["is_error"], false, "the agent run failed: {result}");
    let names = std::fs::read_to_string(env_path("M3S_LIVE_ENV")).unwrap();
    let failures = check_all(
        &env_path("M3S_LIVE_PROJECT"),
        &env_path("M3S_LIVE_BASELINE"),
        &names,
    );
    assert!(
        failures.is_empty(),
        "M3S live outcomes failed:\n- {}",
        failures.join("\n- ")
    );
}

// --- The checker, tested on a scripted twin of the live session ---

fn baseline(root: &Path, to: &Path) {
    for item in CONFIG {
        let (from, dest) = (root.join(item), to.join(item));
        if from.is_dir() {
            concordia_tests::copy_project(&from, &dest).unwrap();
        } else {
            std::fs::copy(&from, &dest).unwrap();
        }
    }
}

fn body(name: &str, kind: &str, y: f32, half: [f32; 3]) -> Value {
    json!({
        "Name": name,
        "Transform": {"translation": [0.0, y, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        "RigidBody": {"kind": kind},
        "Collider": {"shape": {"Box": {"half_extents": half}}},
    })
}

/// What the live team should do, scripted: the director plans, the level designer builds, QA approves.
async fn scripted_session(root: &Path, mcp: &McpClient) {
    let ld = Some("level-designer");
    let task = agent_call(
        root,
        mcp,
        None,
        "task_create",
        json!({"title": "Lit physics playground", "role": "level-designer"}),
    )
    .await;
    let id = task["task"]["id"].as_str().unwrap().to_owned();
    agent_call(root, mcp, ld, "task_claim", json!({"id": id})).await;
    agent_call(
        root,
        mcp,
        ld,
        "scene_new",
        json!({"path": SCENE, "name": "Playground"}),
    )
    .await;
    let mut entities = vec![body("Ground", "Static", 0.0, [10.0, 0.1, 10.0])];
    entities.extend((0..3).map(|i| {
        body(
            &format!("Box {i}"),
            "Dynamic",
            2.0 + i as f32,
            [0.5, 0.5, 0.5],
        )
    }));
    entities.push(json!({"Name": "Sun", "DirectionalLight": {}}));
    entities.push(json!({"Name": "Camera", "Camera": {}, "Transform": {"translation": [0.0, 4.0, 12.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]}}));
    for components in entities {
        agent_call(
            root,
            mcp,
            ld,
            "entity_spawn",
            json!({"components": components}),
        )
        .await;
    }
    agent_call(root, mcp, ld, "scene_save", json!({})).await;
    agent_call(
        root,
        mcp,
        ld,
        "task_update",
        json!({"id": id, "status": "in_review"}),
    )
    .await;
    agent_call(
        root,
        mcp,
        Some("qa-playtester"),
        "task_approve",
        json!({"id": id, "comment": "ground, sun, camera, 3 boxes"}),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn m3s_live_checker_accepts_a_scripted_session_and_rejects_violations() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    let base = dir.path().join("baseline");
    baseline(&root, &base);
    let mcp = mcp(&root).await;
    scripted_session(&root, &mcp).await;
    mcp.shutdown().await;

    let failures = check_all(&root, &base, "PATH\nHOME\n");
    assert!(
        failures.is_empty(),
        "the scripted session should pass:\n- {}",
        failures.join("\n- ")
    );

    assert!(check_env("PATH\nANTHROPIC_API_KEY\n").is_err());
    std::fs::write(root.join("studio/team.toml"), "# edited by an agent\n").unwrap();
    let failures = check_all(&root, &base, "PATH\n");
    assert!(
        failures.iter().any(|f| f.contains("studio changed")),
        "{failures:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn m3s_live_checker_rejects_an_edit_by_the_wrong_actor_and_a_missing_review() {
    let dir = tempfile::tempdir().unwrap();
    let root = new_project(dir.path());
    let base = dir.path().join("baseline");
    baseline(&root, &base);
    let mcp = mcp(&root).await;
    let ld = Some("level-designer");
    let task = agent_call(
        &root,
        &mcp,
        None,
        "task_create",
        json!({"title": "Playground", "role": "level-designer"}),
    )
    .await;
    agent_call(
        &root,
        &mcp,
        ld,
        "task_claim",
        json!({"id": task["task"]["id"]}),
    )
    .await;
    agent_call(
        &root,
        &mcp,
        ld,
        "scene_new",
        json!({"path": SCENE, "name": "Playground"}),
    )
    .await;
    // The tech artist (allowed to edit components) touches the level designer's scene.
    let wall = agent_call(
        &root,
        &mcp,
        ld,
        "entity_spawn",
        json!({"components": {"Name": "Wall"}}),
    )
    .await;
    agent_call(
        &root,
        &mcp,
        Some("tech-artist"),
        "component_set",
        json!({"entity": wall["id"], "component": "Name", "value": "Stray"}),
    )
    .await;
    agent_call(&root, &mcp, ld, "scene_save", json!({})).await;
    mcp.shutdown().await;

    let failures = check_all(&root, &base, "PATH\n");
    assert!(
        failures.iter().any(|f| f.contains("agent:tech-artist")),
        "{failures:?}"
    );
    assert!(
        failures.iter().any(|f| f.contains("QA playtester")),
        "{failures:?}"
    );
    assert!(
        failures.iter().any(|f| f.contains("no static ground")),
        "{failures:?}"
    );
}
