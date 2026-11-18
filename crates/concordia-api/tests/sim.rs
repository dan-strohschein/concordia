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
