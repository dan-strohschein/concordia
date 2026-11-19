//! Team questions and answers (M3 phase 3.6b, ADR-0081): `studio.ask/answer/escalate/questions`.

mod common;

use std::sync::{Arc, Mutex};

use common::Fixture;
use serde_json::{Value, json};

const LD: &str = "agent:level-designer#1";
const QA: &str = "agent:qa-playtester#1";
const DIRECTOR: &str = "agent:director#1";
const HUMAN: &str = "human:local";

fn ask(fx: &mut Fixture, actor: &str, to: &str, text: &str, task: Option<&str>) -> String {
    let mut params = json!({"to": to, "text": text, "actor": actor});
    if let Some(task) = task {
        params["task"] = json!(task);
    }
    fx.ok("studio.ask", params)["question"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn question(fx: &mut Fixture, id: &str) -> Value {
    let all = fx.ok("studio.questions", json!({}));
    all["questions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["id"] == id)
        .unwrap_or_else(|| panic!("{id} not listed: {all}"))
        .clone()
}

fn error_kind(result: Result<Value, concordia_api::ApiError>) -> (String, String) {
    let e = result.expect_err("should fail");
    (e.data.kind.to_owned(), e.message)
}

#[test]
fn ask_records_who_asked_what_and_when() {
    let mut fx = Fixture::new();
    let task = fx.create("arena");
    let before = now_ms();
    let asked = fx.ok(
        "studio.ask",
        json!({"to": "qa-playtester", "text": "ramp angle: 20 or 30?", "task": task, "refs": ["T-0001"], "actor": LD}),
    );
    let q = &asked["question"];
    assert_eq!(q["id"], "Q-0001");
    assert_eq!(q["from"], LD);
    assert_eq!(q["to"], "qa-playtester");
    assert_eq!(q["task"], task.as_str());
    assert_eq!(q["refs"], json!(["T-0001"]));
    assert_eq!(q["text"], "ramp angle: 20 or 30?");
    assert_eq!(q["status"], "open");
    assert!(q["asked_ms"].as_u64().unwrap() >= before, "{q}");
    assert_eq!(question(&mut fx, "Q-0001"), *q);
}

#[test]
fn question_ids_are_sequential_across_engines() {
    let mut fx = Fixture::new();
    assert_eq!(ask(&mut fx, LD, "director", "a?", None), "Q-0001");
    let mut other = common::engine(fx.dir.path());
    let session = other.open_session();
    let second = other
        .call(
            session,
            "studio.ask",
            json!({"to": "director", "text": "b?", "actor": QA}),
        )
        .unwrap();
    assert_eq!(second["question"]["id"], "Q-0002");
    assert_eq!(ask(&mut fx, LD, "director", "c?", None), "Q-0003");
}

#[test]
fn addressed_role_director_or_human_may_answer() {
    let mut fx = Fixture::new();
    for answerer in ["agent:qa-playtester#2", DIRECTOR, HUMAN] {
        let id = ask(&mut fx, LD, "qa-playtester", "which camera?", None);
        let answered = fx.ok(
            "studio.answer",
            json!({"id": id, "text": "Camera-Main", "actor": answerer}),
        );
        let q = &answered["question"];
        assert_eq!(q["status"], "answered", "{answerer}");
        assert_eq!(q["answer"]["by"], answerer);
        assert_eq!(q["answer"]["text"], "Camera-Main");
        assert!(q["answer"]["ms"].as_u64().unwrap() >= q["asked_ms"].as_u64().unwrap());
    }
}

#[test]
fn another_role_may_not_answer() {
    let mut fx = Fixture::new();
    let id = ask(&mut fx, LD, "qa-playtester", "which camera?", None);
    let (kind, message) = error_kind(fx.call(
        "studio.answer",
        json!({"id": id, "text": "mine", "actor": "agent:level-designer#2"}),
    ));
    assert_eq!(kind, "NotPermitted");
    assert!(message.contains("qa-playtester"), "{message}");
    assert_eq!(question(&mut fx, &id)["status"], "open");
}

#[test]
fn text_over_the_cap_is_rejected_with_the_limit() {
    let mut fx = Fixture::new();
    let long = "x".repeat(281);
    let (kind, message) = error_kind(fx.call(
        "studio.ask",
        json!({"to": "director", "text": long, "actor": LD}),
    ));
    assert_eq!(kind, "InvalidParams");
    assert!(message.contains("280"), "{message}");
    let id = ask(&mut fx, LD, "director", "ok?", None);
    let (kind, message) = error_kind(fx.call(
        "studio.answer",
        json!({"id": id, "text": "y".repeat(501), "actor": DIRECTOR}),
    ));
    assert_eq!(kind, "InvalidParams");
    assert!(message.contains("500"), "{message}");
    let (kind, message) = error_kind(fx.call(
        "studio.escalate",
        json!({"id": id, "reason": "z".repeat(281), "actor": DIRECTOR}),
    ));
    assert_eq!(kind, "InvalidParams");
    assert!(message.contains("280"), "{message}");
}

#[test]
fn only_the_director_escalates() {
    let mut fx = Fixture::new();
    let id = ask(&mut fx, LD, "qa-playtester", "art style?", None);
    for actor in [QA, LD, HUMAN] {
        let (kind, _) = error_kind(fx.call(
            "studio.escalate",
            json!({"id": id, "reason": "design call", "actor": actor}),
        ));
        assert_eq!(kind, "NotPermitted", "{actor}");
    }
    let escalated = fx.ok(
        "studio.escalate",
        json!({"id": id, "reason": "design call", "actor": DIRECTOR}),
    );
    let q = &escalated["question"];
    assert_eq!(q["status"], "escalated");
    assert_eq!(q["to"], "human");
    assert_eq!(q["escalation"]["by"], DIRECTOR);
    assert_eq!(q["escalation"]["reason"], "design call");
}

#[test]
fn escalated_questions_take_only_human_answers() {
    let mut fx = Fixture::new();
    let id = ask(&mut fx, LD, "qa-playtester", "art style?", None);
    fx.ok(
        "studio.escalate",
        json!({"id": id, "reason": "design call", "actor": DIRECTOR}),
    );
    for actor in [DIRECTOR, "agent:qa-playtester#2"] {
        let (kind, message) = error_kind(fx.call(
            "studio.answer",
            json!({"id": id, "text": "x", "actor": actor}),
        ));
        assert_eq!(kind, "NotPermitted", "{actor}");
        assert!(message.contains("human"), "{message}");
    }
    let answered = fx.ok(
        "studio.answer",
        json!({"id": id, "text": "low poly", "actor": HUMAN}),
    );
    assert_eq!(answered["question"]["status"], "answered");
}

#[test]
fn escalation_blocks_the_task_and_a_human_answer_unblocks_it() {
    let mut fx = Fixture::new();
    let task = fx.create("arena");
    fx.ok("task.claim", json!({"id": task, "actor": LD}));
    let id = ask(&mut fx, LD, "director", "how big?", Some(&task));
    fx.ok(
        "studio.escalate",
        json!({"id": id, "reason": "scale is a design call", "actor": DIRECTOR}),
    );
    let blocked = fx.ok("task.get", json!({"id": task}));
    assert_eq!(blocked["task"]["status"], "blocked");
    let last = blocked["history"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["actor"], "system:studio", "{last}");

    fx.ok(
        "studio.answer",
        json!({"id": id, "text": "40 m square", "actor": HUMAN}),
    );
    let unblocked = fx.ok("task.get", json!({"id": task}));
    assert_eq!(unblocked["task"]["status"], "todo");
}

#[test]
fn a_question_closes_on_its_first_answer() {
    let mut fx = Fixture::new();
    let id = ask(&mut fx, LD, "director", "go?", None);
    fx.ok(
        "studio.answer",
        json!({"id": id, "text": "yes", "actor": DIRECTOR}),
    );
    let (kind, message) = error_kind(fx.call(
        "studio.answer",
        json!({"id": id, "text": "no", "actor": HUMAN}),
    ));
    assert_eq!(kind, "InvalidParams");
    assert!(message.contains("already answered"), "{message}");
    assert_eq!(question(&mut fx, &id)["answer"]["text"], "yes");
    let (kind, _) = error_kind(fx.call(
        "studio.escalate",
        json!({"id": id, "reason": "late", "actor": DIRECTOR}),
    ));
    assert_eq!(kind, "InvalidParams");
}

#[test]
fn questions_filter_by_status_role_and_task() {
    let mut fx = Fixture::new();
    let task = fx.create("arena");
    let a = ask(&mut fx, LD, "qa-playtester", "a?", Some(&task));
    let b = ask(&mut fx, QA, "director", "b?", None);
    let c = ask(&mut fx, LD, "director", "c?", None);
    fx.ok(
        "studio.answer",
        json!({"id": c, "text": "yes", "actor": DIRECTOR}),
    );
    let ids = |fx: &mut Fixture, filter: Value| -> Vec<String> {
        fx.ok("studio.questions", filter)["questions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| q["id"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(
        ids(&mut fx, json!({"status": "open"})),
        [a.clone(), b.clone()]
    );
    assert_eq!(
        ids(&mut fx, json!({"status": "answered"})),
        std::slice::from_ref(&c)
    );
    assert_eq!(
        ids(&mut fx, json!({"to": "director"})),
        [b.clone(), c.clone()]
    );
    assert_eq!(
        ids(&mut fx, json!({"from": "level-designer"})),
        [a.clone(), c]
    );
    assert_eq!(ids(&mut fx, json!({"from": QA})), [b]);
    assert_eq!(
        ids(&mut fx, json!({"task": task})),
        std::slice::from_ref(&a)
    );
    assert_eq!(ids(&mut fx, json!({"limit": 1})), [a]);
}

#[test]
fn questions_since_returns_only_ones_with_newer_events() {
    let mut fx = Fixture::new();
    let a = ask(&mut fx, LD, "director", "a?", None);
    let first = fx.ok("studio.questions", json!({}));
    let next = first["next"].as_u64().unwrap();
    let b = ask(&mut fx, LD, "director", "b?", None);
    fx.ok(
        "studio.answer",
        json!({"id": a, "text": "yes", "actor": DIRECTOR}),
    );
    let later = fx.ok("studio.questions", json!({"since": next}));
    let ids: Vec<&str> = later["questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [a.as_str(), b.as_str()]);
    assert!(later["next"].as_u64().unwrap() > next);
}

#[test]
fn question_history_survives_a_restart() {
    let mut fx = Fixture::new();
    let id = ask(&mut fx, LD, "director", "go?", None);
    fx.ok(
        "studio.answer",
        json!({"id": id, "text": "yes", "actor": DIRECTOR}),
    );
    let mut restarted = common::engine(fx.dir.path());
    let session = restarted.open_session();
    let listed = restarted
        .call(session, "studio.questions", json!({}))
        .unwrap();
    assert_eq!(listed["questions"][0]["answer"]["text"], "yes");
    assert_eq!(listed["questions"][0]["from"], LD);
}

#[test]
fn ask_answer_and_escalate_publish_studio_events() {
    let mut fx = Fixture::new();
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let sink = Arc::clone(&seen);
    fx.engine.set_event_sink(
        fx.session,
        Box::new(move |event| sink.lock().unwrap().push(event)),
    );
    fx.ok("events.subscribe", json!({"topics": ["studio"]}));
    let id = ask(&mut fx, LD, "director", "go?", None);
    fx.ok(
        "studio.escalate",
        json!({"id": id, "reason": "design call", "actor": DIRECTOR}),
    );
    fx.ok(
        "studio.answer",
        json!({"id": id, "text": "yes", "actor": HUMAN}),
    );
    let kinds: Vec<String> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|e| e["params"]["kind"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        kinds,
        ["question_asked", "question_escalated", "question_answered"]
    );
}

#[test]
fn asking_an_unknown_role_or_task_is_rejected() {
    let mut fx = Fixture::new();
    let (kind, message) = error_kind(fx.call(
        "studio.ask",
        json!({"to": "level-desiner", "text": "a?", "actor": QA}),
    ));
    assert_eq!(kind, "InvalidParams");
    assert!(
        message.contains("level-designer"),
        "did you mean: {message}"
    );
    let (kind, _) = error_kind(fx.call(
        "studio.ask",
        json!({"to": "writer", "text": "a?", "actor": QA}),
    ));
    assert_eq!(kind, "InvalidParams", "writer is disabled");
    let (kind, _) = error_kind(fx.call(
        "studio.ask",
        json!({"to": "director", "text": "a?", "task": "T-0099", "actor": QA}),
    ));
    assert_eq!(kind, "TaskNotFound");
}

#[test]
fn questions_need_team_mode() {
    let dir = tempfile::tempdir().unwrap();
    let mut engine =
        concordia_api::Engine::new(concordia_api::EngineConfig::new(dir.path())).unwrap();
    for name in [
        "studio.ask",
        "studio.answer",
        "studio.escalate",
        "studio.questions",
    ] {
        assert!(engine.methods().get(name).is_none(), "{name}");
    }
    let session = engine.open_session();
    let err = engine
        .call(session, "studio.questions", json!({}))
        .unwrap_err();
    assert_eq!(err.data.kind, "MethodNotFound");
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
