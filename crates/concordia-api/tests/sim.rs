//! Edit world vs play world and the fixed-step simulation (M0 phase 0.6).

mod common;

use std::num::NonZeroUsize;

use common::Harness;
use serde_json::{Value, json};

fn moving(h: &mut Harness) -> String {
    let components = json!({
        "Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        "Velocity": {"linear": [1.0, 0.0, 0.5], "angular": [0.0, 1.0, 0.0]},
    });
    h.ok("entity.spawn", json!({ "components": components }))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn x(value: &Value) -> f64 {
    value["value"]["translation"][0].as_f64().unwrap()
}

#[test]
fn sim_step_returns_hash_and_leaves_edit_world_untouched() {
    let mut h = Harness::new().ready();
    let id = moving(&mut h);
    let edit_before = h.ok("scene.info", json!({}));
    let step = h.ok("sim.step", json!({"ticks": 60}));
    assert_eq!(step["tick"], 60);
    assert_eq!(step["world_hash"].as_str().unwrap().len(), 64);
    let play = h.ok(
        "component.get",
        json!({"entity": id, "component": "Transform", "world": "play"}),
    );
    assert!((x(&play) - 1.0).abs() < 1e-4, "{play}");
    let edit = h.ok(
        "component.get",
        json!({"entity": id, "component": "Transform"}),
    );
    assert_eq!(x(&edit), 0.0);
    assert_eq!(h.ok("scene.info", json!({})), edit_before);
}

#[test]
fn sim_step_hash_same_for_threads_1_and_8() {
    let hash = |threads: usize| {
        let dir = tempfile::tempdir().unwrap();
        let mut h =
            Harness::with_config(dir, |c| c.threads = NonZeroUsize::new(threads).unwrap()).ready();
        for _ in 0..50 {
            moving(&mut h);
        }
        h.ok("sim.step", json!({"ticks": 600}))["world_hash"].clone()
    };
    assert_eq!(hash(1), hash(8));
}

#[test]
fn play_world_is_rebuilt_from_edit_world_after_stop() {
    let mut h = Harness::new().ready();
    let id = moving(&mut h);
    h.ok("sim.step", json!({"ticks": 30}));
    let stopped = h.ok("sim.stop", json!({}));
    assert_eq!(stopped["state"], "stopped");
    let err = h.err(
        "component.get",
        json!({"entity": id, "component": "Transform", "world": "play"}),
    );
    assert_eq!((err.code(), err.data.kind), (-32011, "NotPlaying"));
    let again = h.ok("sim.step", json!({"ticks": 1}));
    assert_eq!(again["tick"], 1);
}

#[test]
fn sim_play_and_pause_report_state() {
    let mut h = Harness::new().ready();
    moving(&mut h);
    assert_eq!(h.ok("sim.play", json!({}))["state"], "playing");
    std::thread::sleep(std::time::Duration::from_millis(60));
    let paused = h.ok("sim.pause", json!({}));
    assert_eq!(paused["state"], "paused");
    assert!(paused["tick"].as_u64().unwrap() >= 1, "{paused}");
    let err = h.err("sim.pause", json!({"bogus": 1}));
    assert_eq!(err.code(), -32602);
}

#[test]
fn sim_step_accepts_optional_actor_and_logs_attribution() {
    use std::sync::Arc;

    use concordia_core::log::LogRing;
    use tracing_subscriber::layer::SubscriberExt;

    let ring = Arc::new(LogRing::new(NonZeroUsize::new(64).unwrap()));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let dir = tempfile::tempdir().unwrap();
        let shared = Arc::clone(&ring);
        let mut h = Harness::with_config(dir, move |c| c.log_ring = Some(shared)).ready();
        moving(&mut h);
        let step = h.ok("sim.step", json!({"ticks": 3, "actor": "agent:gate"}));
        let records = h.ok("log.tail", json!({"level": "info"}))["records"].clone();
        let stepped = records
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["message"] == "sim stepped")
            .unwrap_or_else(|| panic!("no 'sim stepped' record: {records}"));
        assert_eq!(stepped["fields"]["actor"], "agent:gate");
        assert_eq!(stepped["fields"]["ticks"], "3");
        assert_eq!(stepped["fields"]["world_hash"], step["world_hash"]);
        // Attribution only: nothing is journaled.
        let history = h.ok("txn.history", json!({}));
        assert!(
            history["entries"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| !e.to_string().contains("sim")),
            "{history}"
        );
        for method in ["sim.play", "sim.pause", "sim.stop"] {
            h.ok(method, json!({"actor": "agent:gate"}));
        }
        let err = h.err("sim.step", json!({"ticks": 1, "actor": "not an actor"}));
        assert_eq!(err.data.kind, "ActorMalformed");
    });
}

#[test]
fn sim_step_without_actor_uses_session_default() {
    let mut h = Harness::new().ready();
    moving(&mut h);
    let step = h.ok("sim.step", json!({"ticks": 1}));
    assert_eq!(step["tick"], 1);
}
