//! Evaluating playtest assertions (M3 phase 3.2b): over a replay, over a `.playtest.ron`
//! scenario, and live on the current play world.
//!
//! The rig's movers start at x = 0, 1, 2 (z = 0); "Mover 0" is tagged `player`. A "Door" is at
//! (0, 0, 10), tagged `door`, with no Velocity, so it never moves. W is pressed from tick 0, so
//! `camera_pan` is (0, 1) and steer sets every Velocity to (0, 0, 2) m/s on tick 0, before
//! velocity is integrated on the same tick: after t ticks, z = 2 t / 60 = t / 30 m.
//!
//! - The player is within 2 m of the door when 10 − t/30 < 2, i.e. t > 240: tick 241, at a
//!   distance of 10 − 241/30 = 1.9667 m.
//! - Within 120 ticks it never is; the closest approach is at tick 120: 10 − 120/30 = 6 m.
//! - `always: name("Mover 0").Transform.translation.z < 1` fails when t/30 ≥ 1: tick 30 (f32
//!   lands on 1.0000004).

mod common;

use common::Rig;
use serde_json::{Value, json};

const PATH: &str = "tests/replays/walk.replay.ron";
const REACH: &str = r#"within 300 ticks: distance(tag("player"), tag("door")) < 2m"#;

/// The rig, its door and tags, and a 300-tick recording holding W.
fn walked() -> Rig {
    let mut rig = Rig::new();
    let mover = rig.ok(
        "query.run",
        json!({"with": ["Name"], "where": [{"path": "Name", "op": "eq", "value": "Mover 0"}]}),
    )["rows"][0]["id"]
        .clone();
    rig.ok(
        "component.set",
        json!({"entity": mover, "component": "Tags", "value": ["player"]}),
    );
    rig.ok(
        "entity.spawn",
        json!({"components": {
            "Name": "Door",
            "Tags": ["door"],
            "Transform": {"translation": [0.0, 0.0, 10.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        }}),
    );
    rig.ok("scene.save", json!({}));
    let hold_w = json!([{"tick": 0, "event": {"Key": {"key": "W", "pressed": true}}}]);
    rig.record(PATH, 60, hold_w, 300);
    rig
}

fn eval(rig: &mut Rig, assertions: &[&str]) -> Value {
    rig.ok(
        "assert.eval",
        json!({"assertions": assertions, "source": "replay", "path": PATH}),
    )
}

fn near(v: &Value, want: f64) -> bool {
    (v.as_f64().unwrap() - want).abs() < 1e-3
}

#[test]
fn within_passes_at_first_satisfying_tick_and_reports_it() {
    let mut rig = walked();
    let r = &eval(&mut rig, &[REACH])["results"][0];
    assert_eq!(
        (&r["status"], &r["decided_at"]),
        (&json!("passed"), &json!(241)),
        "{r}"
    );
    let key = r#"distance(tag("player"), tag("door"))"#;
    assert!(near(&r["witness"][key], 10.0 - 241.0 / 30.0), "{r}");
}

#[test]
fn within_fails_at_deadline_with_closest_approach_witness() {
    let mut rig = walked();
    let r = &eval(
        &mut rig,
        &[r#"within 120 ticks: distance(tag("player"), tag("door")) < 2m"#],
    )["results"][0];
    assert_eq!(
        (&r["status"], &r["decided_at"]),
        (&json!("failed"), &json!(120)),
        "{r}"
    );
    assert!(near(&r["witness"]["closest"], 6.0), "{r}");
    assert_eq!(r["witness"]["closest_at"], 120);
}

#[test]
fn always_fails_at_first_violation_and_never_passes_when_event_absent() {
    let mut rig = walked();
    let out = eval(
        &mut rig,
        &[
            r#"always: name("Mover 0").Transform.translation.z < 1"#,
            r#"never: event("PlayerDied")"#,
            r#"eventually: count(tag("door")) == 2"#,
            r#"at 60 ticks: all(with(Velocity)).Transform.translation.z > 1.9 and exists(tag("player"))"#,
            r#"always: any(tag("player")).Transform.translation.x == 0 and not exists(name("Ghost"))"#,
            r#"at 60 ticks: any(with(Velocity)).Transform.translation.x < 1"#,
            r#"at 60 ticks: all(with(Velocity)).Transform.translation.x < 1"#,
        ],
    );
    let status: Vec<(&Value, &Value)> = out["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (&r["status"], &r["decided_at"]))
        .collect();
    // always fails at 30; never holds for the whole replay (decided at its end, 300); the door
    // never doubles, so eventually is undecided; at 60 every mover is at 60/30 = 2 > 1.9; the
    // movers sit at x = 0, 1, 2, so any is below 1 but not all.
    assert_eq!(
        status,
        [
            (&json!("failed"), &json!(30)),
            (&json!("passed"), &json!(300)),
            (&json!("undecided"), &Value::Null),
            (&json!("passed"), &json!(60)),
            (&json!("passed"), &json!(300)),
            (&json!("passed"), &json!(60)),
            (&json!("failed"), &json!(60)),
        ],
        "{out}"
    );
    assert_eq!(out["ok"], false, "one failed");
}

/// A scenario with the same inputs as the recording decides the same way on the same tick.
#[test]
fn evaluation_over_replay_equals_evaluation_over_a_scenario() {
    let mut rig = walked();
    let file = rig.dir.path().join("tests/playtests/walk.playtest.ron");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        format!(
            "Playtest(\n    scene: \"scenes/test.scene.ron\",\n    max_ticks: 300,\n    inputs: [(tick: Some(0), event: Key(key: W, pressed: true))],\n    assertions: [{REACH:?}],\n)\n"
        ),
    )
    .unwrap();
    let scenario = rig.ok(
        "assert.eval",
        json!({"source": "scenario", "path": "tests/playtests/walk.playtest.ron"}),
    );
    let replay = eval(&mut rig, &[REACH]);
    assert_eq!(scenario["results"], replay["results"], "{scenario}");
}

/// Live checks the predicate on the current play world now; nothing changes.
#[test]
fn assert_eval_live_has_no_side_effects_on_the_world() {
    let mut rig = walked();
    rig.ok("sim.step", json!({"ticks": 5}));
    let play = |rig: &Rig| {
        rig.engine
            .world(concordia_api::WorldSel::play)
            .unwrap()
            .state_hash()
            .unwrap()
    };
    let before = play(&rig);
    let edit = rig
        .engine
        .world(concordia_api::WorldSel::edit)
        .unwrap()
        .state_hash()
        .unwrap();
    let r = rig.ok(
        "assert.eval",
        json!({"assertions": [r#"always: exists(tag("door"))"#]}),
    );
    assert_eq!(r["results"][0]["status"], "passed", "{r}");
    assert_eq!(play(&rig), before);
    assert_eq!(
        rig.engine
            .world(concordia_api::WorldSel::edit)
            .unwrap()
            .state_hash()
            .unwrap(),
        edit
    );
}

/// Text that doesn't parse or check is refused before anything runs, with the reason.
#[test]
fn assert_eval_refuses_bad_text_naming_the_problem() {
    let mut rig = walked();
    let err = rig.err(
        "assert.eval",
        json!({"assertions": [r#"within 600 tickz: event("A")"#], "source": "replay", "path": PATH}),
    );
    assert_eq!(
        err.kind,
        concordia_api::ApiErrorKind::InvalidParams,
        "{err:?}"
    );
    assert!(err.message.contains("tickz"), "{err:?}");
    let err = rig.err(
        "assert.eval",
        json!({"assertions": [r#"always: name("x").Transfrom.translation.x > 0"#]}),
    );
    assert!(err.message.contains("Transform"), "did you mean: {err:?}");
}
