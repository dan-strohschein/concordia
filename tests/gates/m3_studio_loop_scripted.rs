//! M3 gate: the closed loop without a model. Three scripted agents (the director, the gameplay
//! programmer and QA) work T-0001 on a copy of the fixture, every engine call and file edit going
//! through the real `concordia studio hook` binary and the real `concordia mcp`, exactly as
//! Claude Code would make them. The shared checker (`m3_checker`) then accepts the result, and
//! rejects a project where the loop never ran.
//!
//! By hand: the fixed module walks the player to within 2 m of the door at tick 88 (see
//! `m3_replay_assert`); the seeded bug never gets there in 600 ticks.

use std::path::PathBuf;

use concordia_tests::m3_checker::{REACH, TASK, baseline, check_all};
use concordia_tests::m3_fixture;
use concordia_tests::mcp_client::McpClient;
use concordia_tests::studio_gate::{
    agent_call, agent_edit, agent_try, decision, mcp, pre_tool_use, studio,
};
use serde_json::{Value, json};

const GP: Option<&str> = Some("gameplay-programmer");
const QA: Option<&str> = Some("qa-playtester");
const LIB: &str = "scripts/player_controller/src/lib.rs";
const REPLAY: &str = "tests/replays/T-0001-walk-to-door.replay.ron";

/// A built, synced copy of the fixture, its post-sync configuration, and a server on it.
struct Loop {
    dir: tempfile::TempDir,
    root: PathBuf,
    mcp: McpClient,
}

impl Loop {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = m3_fixture::built(&dir.path().join("game"));
        let synced = studio(&root, &["sync"]);
        assert!(
            synced.status.success(),
            "{}",
            String::from_utf8_lossy(&synced.stderr)
        );
        baseline(&root, &dir.path().join("baseline"));
        let mcp = mcp(&root).await;
        Self { dir, root, mcp }
    }

    async fn call(&self, role: Option<&str>, tool: &str, args: Value) -> Value {
        agent_call(&self.root, &self.mcp, role, tool, args).await
    }

    async fn task(&self) -> Value {
        self.call(QA, "task_get", json!({"id": TASK})).await["task"].clone()
    }
}

/// The director hands T-0001 to the gameplay programmer.
async fn director_assigns(l: &Loop) {
    l.call(
        None,
        "task_update",
        json!({"id": TASK, "assignee": "agent:gameplay-programmer#1"}),
    )
    .await;
    l.call(
        None,
        "task_comment",
        json!({"id": TASK, "text": "Gameplay: fix the walk; QA proves it with a replay."}),
    )
    .await;
}

/// The gameplay programmer claims it, is refused outside its paths, fixes the module (edit,
/// build, reload), checks the playtest and hands it in.
async fn gameplay_fixes(l: &Loop) {
    l.call(GP, "task_claim", json!({"id": TASK})).await;
    assert_eq!(
        agent_edit(&l.root, GP, "scenes/main.scene.ron", "broken"),
        "deny"
    );
    assert_eq!(
        agent_edit(&l.root, GP, "studio/team.toml", "# mine now\n"),
        "deny"
    );
    let fixed = l.dir.path().join("fixed");
    m3_fixture::copy(&fixed);
    m3_fixture::apply_fix(&fixed);
    let text = std::fs::read_to_string(fixed.join(LIB)).unwrap();
    assert_eq!(agent_edit(&l.root, GP, LIB, &text), "allow");
    let build = json!({"command": "cargo build --release --target wasm32-wasip2"});
    let out = pre_tool_use(&l.root, GP, "agent-gameplay-programmer", "Bash", build);
    assert_eq!(decision(&out), "allow", "{out}");
    m3_fixture::build(&l.root);
    l.call(GP, "script_reload", json!({})).await;
    l.call(GP, "scene_open", json!({"path": "scenes/main.scene.ron"}))
        .await;
    let check = l
        .call(
            GP,
            "assert_eval",
            json!({"source": "scenario", "path": "tests/playtests/walk-to-door.playtest.ron"}),
        )
        .await;
    assert_eq!(check["results"][0]["status"], "passed", "{check}");
    l.call(GP, "task_comment", json!({"id": TASK, "text": "step() applied dt twice; fixed in scripts/player_controller. The playtest passes at tick 88."}))
        .await;
    l.call(
        GP,
        "task_update",
        json!({"id": TASK, "status": "in_review"}),
    )
    .await;
}

/// QA records the walk, proves the acceptance over it, names it in the task and approves.
async fn qa_proves(l: &Loop) {
    l.call(QA, "scene_open", json!({"path": "scenes/main.scene.ron"}))
        .await;
    l.call(
        QA,
        "replay_record",
        json!({"action": "start", "path": REPLAY, "task": TASK}),
    )
    .await;
    let hold = json!([{"tick": 0, "event": {"Key": {"key": "W", "pressed": true}}}]);
    l.call(QA, "input_inject", json!({"events": hold})).await;
    l.call(QA, "sim_step", json!({"ticks": 600})).await;
    l.call(QA, "replay_record", json!({"action": "stop"})).await;
    let proof = l
        .call(
            QA,
            "assert_eval",
            json!({"assertions": [REACH], "source": "replay", "path": REPLAY}),
        )
        .await;
    assert_eq!(proof["results"][0]["status"], "passed", "{proof}");
    l.call(QA, "task_comment", json!({"id": TASK, "text": format!("Replay {REPLAY}: the player reaches the door at tick 88.")}))
        .await;
    l.call(
        QA,
        "task_approve",
        json!({"id": TASK, "comment": "proved by the replay"}),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn scripted_director_assigns_task_to_gameplay() {
    let l = Loop::new().await;
    director_assigns(&l).await;
    let task = l.task().await;
    assert_eq!(task["assignee"], "agent:gameplay-programmer#1", "{task}");
    assert_eq!(task["status"], "todo");
    l.mcp.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn scripted_gameplay_edit_outside_scope_is_blocked_and_inside_scope_applies_fix_patch() {
    let l = Loop::new().await;
    director_assigns(&l).await;
    gameplay_fixes(&l).await;
    assert_eq!(
        std::fs::read_to_string(l.root.join("studio/team.toml")).unwrap(),
        std::fs::read_to_string(l.dir.path().join("baseline/studio/team.toml")).unwrap()
    );
    assert!(
        std::fs::read_to_string(l.root.join(LIB))
            .unwrap()
            .contains("axis * speed * dt\n")
    );
    assert_eq!(l.task().await["status"], "in_review");
    l.mcp.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn scripted_gameplay_cannot_mark_own_task_done() {
    let l = Loop::new().await;
    director_assigns(&l).await;
    gameplay_fixes(&l).await;
    let approve = pre_tool_use(
        &l.root,
        GP,
        "agent-gameplay-programmer",
        "mcp__concordia__task_approve",
        json!({"id": TASK}),
    );
    assert_eq!(
        decision(&approve),
        "deny",
        "the author may not approve: {approve}"
    );
    let done = agent_try(
        &l.root,
        &l.mcp,
        GP,
        "task_update",
        json!({"id": TASK, "status": "done"}),
    )
    .await;
    assert!(done.is_err(), "nor mark it done: {done:?}");
    assert_eq!(l.task().await["status"], "in_review");
    l.mcp.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn scripted_qa_cannot_edit_scripts() {
    let l = Loop::new().await;
    assert_eq!(agent_edit(&l.root, QA, LIB, "// QA was here\n"), "deny");
    assert!(
        !std::fs::read_to_string(l.root.join(LIB))
            .unwrap()
            .contains("QA was here")
    );
    l.mcp.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn scripted_qa_records_replay_asserts_and_marks_done() {
    let l = Loop::new().await;
    director_assigns(&l).await;
    gameplay_fixes(&l).await;
    qa_proves(&l).await;
    let task = l.task().await;
    assert_eq!(task["status"], "done", "{task}");
    let replay = std::fs::read_to_string(l.root.join(REPLAY)).unwrap();
    assert!(replay.contains("agent:qa-playtester"), "QA recorded it");
    l.mcp.shutdown().await;
}

/// The whole loop passes the shared checker; a fresh copy where nothing happened fails it on the
/// task, the edits and the replay; and an edited team file fails integrity.
#[tokio::test(flavor = "multi_thread")]
async fn scripted_loop_passes_m3_checker() {
    let l = Loop::new().await;
    director_assigns(&l).await;
    gameplay_fixes(&l).await;
    qa_proves(&l).await;
    l.mcp.shutdown().await;
    let pristine = m3_fixture::built(&l.dir.path().join("pristine"));
    let base = l.dir.path().join("baseline");
    let failures = check_all(&l.root, &base, &pristine);
    assert!(
        failures.is_empty(),
        "the scripted loop should pass:\n- {}",
        failures.join("\n- ")
    );

    let idle = Loop::new().await;
    idle.mcp.shutdown().await;
    let failures = check_all(&idle.root, &idle.dir.path().join("baseline"), &pristine);
    for needle in [
        "nobody claimed",
        "never edited the player module",
        "tests/replays",
    ] {
        assert!(
            failures.iter().any(|f| f.contains(needle)),
            "{needle}: {failures:?}"
        );
    }
    std::fs::write(l.root.join("studio/team.toml"), "# edited by an agent\n").unwrap();
    let failures = check_all(&l.root, &base, &pristine);
    assert!(
        failures.iter().any(|f| f.contains("studio changed")),
        "{failures:?}"
    );
}
