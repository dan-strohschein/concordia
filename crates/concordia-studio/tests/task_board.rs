//! The task board through the Engine API: `task.*`, files, the state machine and gates (M3 phase 3.3).

mod common;

use common::Fixture;
use concordia_api::ApiErrorKind;
use serde_json::{Value, json};

fn activity(fx: &mut Fixture, filter: Value) -> Vec<Value> {
    fx.ok("studio.activity", filter)["entries"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn task_create_writes_canonical_task_md() {
    let mut fx = Fixture::new();
    let created = fx.ok(
        "task.create",
        json!({
            "title": "Build the arena",
            "role": "level-designer",
            "description": "A ring of eight walls around a lit floor.",
            "priority": "high",
            "acceptance": [{"assert": "the arena has 8 walls"}],
            "actor": "agent:director"
        }),
    );
    assert_eq!(created["task"]["id"], "T-0001");
    assert_eq!(created["task"]["status"], "todo");
    let text =
        std::fs::read_to_string(fx.path("studio/tasks/T-0001-build-the-arena.task.md")).unwrap();
    insta::assert_snapshot!(text);
}

#[test]
fn task_create_for_unknown_role_is_rejected_with_suggestion() {
    let mut fx = Fixture::new();
    let err = fx
        .call(
            "task.create",
            json!({"title": "x", "role": "level-desiner", "actor": "agent:director"}),
        )
        .unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::InvalidParams);
    assert!(
        err.data
            .suggestions
            .iter()
            .any(|s| s.contains("level-designer")),
        "{:?}",
        err.data.suggestions
    );
}

#[test]
fn task_create_for_disabled_role_is_rejected() {
    let mut fx = Fixture::new();
    let err = fx
        .call(
            "task.create",
            json!({"title": "x", "role": "writer", "actor": "agent:director"}),
        )
        .unwrap_err();
    assert!(err.message.contains("disabled"), "{}", err.message);
}

#[test]
fn task_claim_sets_assignee_and_in_progress_and_logs_actor() {
    let mut fx = Fixture::new();
    let id = fx.create("Build the arena");
    let claimed = fx.ok(
        "task.claim",
        json!({"id": id, "actor": "agent:level-designer#1"}),
    );
    assert_eq!(claimed["task"]["status"], "in_progress");
    assert_eq!(claimed["task"]["assignee"], "agent:level-designer#1");
    let entries = activity(&mut fx, json!({"task": id, "kind": "task_changed"}));
    let last = entries.last().unwrap();
    assert_eq!(last["actor"], "agent:level-designer#1");
    assert_eq!(last["changes"]["status"], json!(["todo", "in_progress"]));
}

#[test]
fn illegal_transition_is_rejected() {
    let mut fx = Fixture::new();
    let id = fx.create("Build the arena");
    let err = fx
        .call(
            "task.update",
            json!({"id": id, "status": "done", "actor": "agent:director"}),
        )
        .unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::TaskTransition);
    assert!(err.message.contains("todo → done"), "{}", err.message);
    assert!(
        err.data
            .suggestions
            .iter()
            .any(|s| s.contains("in_progress")),
        "{:?}",
        err.data.suggestions
    );
}

#[test]
fn done_requires_gate_actor() {
    let mut fx = Fixture::new();
    let id = fx.create("Build the arena");
    fx.submit(&id, "agent:level-designer#1");
    let err = fx
        .call(
            "task.approve",
            json!({"id": id, "actor": "agent:level-designer#1"}),
        )
        .unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::NotPermitted);
    assert!(
        err.message.contains("role:qa-playtester") && err.message.contains("human"),
        "{}",
        err.message
    );
    let done = fx.ok(
        "task.approve",
        json!({"id": id, "comment": "8 walls, lit", "actor": "agent:qa-playtester#1"}),
    );
    assert_eq!(done["task"]["status"], "done");
}

#[test]
fn reject_requires_comment() {
    let mut fx = Fixture::new();
    let id = fx.create("Build the arena");
    fx.submit(&id, "agent:level-designer#1");
    let err = fx
        .call(
            "task.reject",
            json!({"id": id, "comment": " ", "actor": "agent:qa-playtester#1"}),
        )
        .unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::InvalidParams);
}

#[test]
fn three_rejections_escalate_to_blocked_with_human_assignee() {
    let mut fx = Fixture::new();
    let id = fx.create("Build the arena");
    for attempt in 1..=3 {
        fx.submit(&id, "agent:level-designer#1");
        let rejected = fx.ok(
            "task.reject",
            json!({"id": id, "comment": format!("walls missing ({attempt})"), "actor": "agent:qa-playtester#1"}),
        );
        let expected = if attempt < 3 { "rejected" } else { "blocked" };
        assert_eq!(rejected["task"]["status"], expected, "attempt {attempt}");
    }
    let task = fx.ok("task.get", json!({"id": id}))["task"].clone();
    assert_eq!(task["assignee"], "human:local");
}

#[test]
fn task_history_is_reconstructed_from_command_log_not_file() {
    let mut fx = Fixture::new();
    let id = fx.create("Build the arena");
    fx.ok(
        "task.claim",
        json!({"id": id, "actor": "agent:level-designer#1"}),
    );
    fx.ok(
        "task.comment",
        json!({"id": id, "text": "walls placed", "actor": "agent:level-designer#1"}),
    );
    let file =
        std::fs::read_to_string(fx.path("studio/tasks/T-0001-build-the-arena.task.md")).unwrap();
    // The file holds the current state only: no record of the todo → in_progress transition.
    assert!(
        file.contains("status: in_progress") && !file.contains("todo"),
        "the file holds no history:\n{file}"
    );

    // A fresh engine rebuilds the same history from the Studio journal.
    let mut engine = common::engine(fx.dir.path());
    let session = engine.open_session();
    let history = engine.call(session, "task.get", json!({"id": id})).unwrap()["history"].clone();
    let actors: Vec<&str> = history
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["actor"].as_str().unwrap())
        .collect();
    assert_eq!(
        actors,
        [
            "agent:director",
            "agent:level-designer#1",
            "agent:level-designer#1"
        ]
    );
}

#[test]
fn hand_edited_task_file_is_reloaded_and_diff_logged_as_human() {
    let mut fx = Fixture::new();
    let id = fx.create("Build the arena");
    let path = fx.path("studio/tasks/T-0001-build-the-arena.task.md");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("priority: normal", "priority: urgent")).unwrap();
    assert_eq!(
        fx.ok("task.get", json!({"id": id}))["task"]["priority"],
        "urgent"
    );
    let entries = activity(&mut fx, json!({"actor": "human:local"}));
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(
        entries[0]["changes"]["priority"],
        json!(["normal", "urgent"])
    );
}

#[test]
fn task_ids_are_stable_and_sequential_under_concurrent_creates() {
    let dir = tempfile::tempdir().unwrap();
    common::write_studio(dir.path());
    let root = dir.path().to_owned();
    let workers: Vec<_> = (0..2)
        .map(|w| {
            let root = root.clone();
            std::thread::spawn(move || {
                let mut engine = common::engine(&root);
                let s = engine.open_session();
                (0..10)
                    .map(|i| {
                        let params = json!({"title": format!("task {w}-{i}"), "role": "level-designer", "actor": "agent:director"});
                        engine.call(s, "task.create", params).unwrap()["task"]["id"]
                            .as_str()
                            .unwrap()
                            .to_owned()
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let mut ids: Vec<String> = workers
        .into_iter()
        .flat_map(|w| w.join().unwrap())
        .collect();
    ids.sort();
    let expected: Vec<String> = (1..=20).map(|n| format!("T-{n:04}")).collect();
    assert_eq!(ids, expected);
}

#[test]
fn task_list_filters_by_status_and_role() {
    let mut fx = Fixture::new();
    let a = fx.create("one");
    fx.create("two");
    fx.ok(
        "task.claim",
        json!({"id": a, "actor": "agent:level-designer#1"}),
    );
    let listed = fx.ok("task.list", json!({"status": "todo"}))["tasks"].clone();
    let titles: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["two"]);
    assert_eq!(
        fx.ok("task.list", json!({"role": "qa-playtester"}))["tasks"],
        json!([])
    );
}

#[test]
fn unknown_task_is_task_not_found() {
    let mut fx = Fixture::new();
    let err = fx.call("task.get", json!({"id": "T-0042"})).unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::TaskNotFound);
}

#[test]
fn task_writes_require_an_actor() {
    let mut fx = Fixture::new();
    let err = fx
        .call(
            "task.create",
            json!({"title": "x", "role": "level-designer"}),
        )
        .unwrap_err();
    assert_eq!(err.kind, ApiErrorKind::ActorRequired);
}
