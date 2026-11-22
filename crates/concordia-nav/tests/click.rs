//! Click-to-move (M2 phase 2.5b.2a, ADR-0078): `ClickToMove` turns a click's pointer ray into a
//! `NavAgent` target, through a physics raycast and a snap onto the navmesh.
//!
//! The floor (`common::engine`) has its top at y = 0 and is walkable to ±9.5 at y = 0.2 (the
//! default volume: 0.25 m cells, a 0.3 m radius, rounded up to 2 cells = 0.5 m of erosion).
//! Physics queries see the bodies as of the last step, so clicks start on tick 1.
//! - A ray straight down at (3, −4) hits the floor's top at (3, 0, −4) and snaps up to the surface
//!   (3, 0.2, −4).
//! - The wall test uses a volume with `edge_max_error` 0.5, whose outlines are exact to the cell
//!   (2.5a: the default 1.3 cuts edges diagonally). Its wall box, half (0.5, 1, 3.05) at
//!   (2.05, 1, 0), covers x ∈ [1.55, 2.55], so it fills the cells from x = 1.5 to 2.75 (faces off
//!   cell boundaries). Eroded by 2 cells, the floor is walkable to x = 1.0 on the left and from
//!   x = 3.25 on the right. A click on the wall's top at (2.25, 0) is 1.0 m from the right edge and
//!   1.25 m from the left, so it snaps to (3.25, 0.2, 0) (the surface is within the snap's 2 m
//!   vertical reach of the top at y = 2).

mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;

use common::engine::{Rig, near};
use concordia_core::log::LogRing;
use serde_json::{Value, json};
use tracing_subscriber::layer::SubscriberExt;

const MAP: &str = r#"InputMap(
    actions: {
        "aim": (kind: Ray, bindings: [PointerRay]),
        "go": (kind: Button, bindings: [MouseButton(Left)]),
    },
)"#;

fn rig() -> Rig {
    Rig::with_input(MAP, None)
}

/// A hero at (−5, 0, 0) that moves where `go` clicks `aim`.
fn hero(rig: &mut Rig, click: Value) -> String {
    let id = rig.character(-5.0, 0.0, json!({"max_speed": 3.0}));
    rig.ok(
        "component.set",
        json!({"entity": id, "component": "ClickToMove", "value": click}),
    );
    id
}

fn click_to_move() -> Value {
    json!({"click": "go", "pointer": "aim", "hold": false, "layers": [], "max_distance": 1000.0})
}

fn down_at(x: f32, z: f32) -> Value {
    json!({"PointerRay": {"origin": [x, 10.0, z], "direction": [0.0, -1.0, 0.0]}})
}

fn press(pressed: bool) -> Value {
    json!({"MouseButton": {"button": "Left", "pressed": pressed}})
}

fn inject(rig: &mut Rig, events: Vec<Value>) {
    let events: Vec<Value> = events.into_iter().map(|e| json!({"event": e})).collect();
    rig.ok("input.inject", json!({ "events": events }));
}

fn target(rig: &mut Rig, id: &str) -> Value {
    rig.play(id, "NavAgent")["target"].clone()
}

#[test]
fn click_sets_the_target_to_the_snapped_hit() {
    let mut rig = rig();
    rig.floor();
    let id = hero(&mut rig, click_to_move());
    rig.step(1);
    assert_eq!(target(&mut rig, &id), Value::Null);
    inject(&mut rig, vec![down_at(3.0, -4.0), press(true)]);
    rig.step(1);
    let t = target(&mut rig, &id);
    assert!(near(&t, [3.0, 0.2, -4.0], 1e-3), "{t}");
    let state = rig.play(&id, "NavAgentState");
    assert_eq!(state["status"], "Moving", "planned the same tick: {state}");
    assert!(
        near(&state["planned_for"], [3.0, 0.2, -4.0], 1e-3),
        "{state}"
    );
    // Without a new click, a new ray changes nothing (hold is off).
    inject(&mut rig, vec![down_at(-2.0, 5.0)]);
    rig.step(1);
    assert!(near(&target(&mut rig, &id), [3.0, 0.2, -4.0], 1e-3));
    // A fresh click uses the latest ray.
    inject(&mut rig, vec![press(false)]);
    rig.step(1);
    inject(&mut rig, vec![press(true)]);
    rig.step(1);
    assert!(near(&target(&mut rig, &id), [-2.0, 0.2, 5.0], 1e-3));
}

#[test]
fn click_on_a_wall_snaps_to_the_nearest_walkable_point() {
    let mut rig = rig();
    rig.spawn(json!({
        "Transform": Rig::at(0.0, -0.5, 0.0),
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [10.1, 0.5, 10.1]}}},
    }));
    rig.spawn(json!({
        "Transform": Rig::at(0.0, 0.0, 0.0),
        "NavMeshVolume": {"half_extents": [12.0, 3.0, 12.0], "edge_max_error": 0.5},
    }));
    rig.spawn(json!({
        "Transform": Rig::at(2.05, 1.0, 0.0),
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [0.5, 1.0, 3.05]}}},
    }));
    let id = hero(&mut rig, click_to_move());
    rig.step(1);
    inject(&mut rig, vec![down_at(2.25, 0.0), press(true)]);
    rig.step(1);
    let t = target(&mut rig, &id);
    assert!(near(&t, [3.25, 0.2, 0.0], 1e-3), "{t}");
}

#[test]
fn click_on_nothing_changes_nothing() {
    // An upward ray hits nothing; a box at x = 30 is hit, but it is outside the 24 m volume, so
    // no navmesh is near.
    let mut rig = rig();
    rig.floor();
    rig.spawn(json!({
        "Transform": Rig::at(30.0, -0.5, 0.0),
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [2.0, 0.5, 2.0]}}},
    }));
    let id = hero(&mut rig, click_to_move());
    rig.step(1);
    inject(&mut rig, vec![down_at(3.0, -4.0), press(true)]);
    rig.step(1);
    let first = target(&mut rig, &id);
    let up = json!({"PointerRay": {"origin": [0.0, 5.0, 0.0], "direction": [0.0, 1.0, 0.0]}});
    for ray in [up, down_at(30.0, 0.0)] {
        inject(&mut rig, vec![press(false)]);
        rig.step(1);
        inject(&mut rig, vec![ray.clone(), press(true)]);
        rig.step(1);
        assert_eq!(target(&mut rig, &id), first, "{ray}");
    }
    let log = rig.ok("log.tail", json!({"level": "warn", "limit": 100}));
    assert!(
        log["records"].as_array().is_none_or(|r| r.is_empty()),
        "a miss isn't a warning: {log}"
    );
}

#[test]
fn hold_retargets_while_pressed() {
    let mut rig = rig();
    rig.floor();
    let mut held = click_to_move();
    held["hold"] = json!(true);
    let id = hero(&mut rig, held);
    rig.step(1);
    inject(&mut rig, vec![down_at(3.0, -4.0), press(true)]);
    rig.step(1);
    assert!(near(&target(&mut rig, &id), [3.0, 0.2, -4.0], 1e-3));
    inject(&mut rig, vec![down_at(4.0, 1.0)]);
    rig.step(1);
    assert!(
        near(&target(&mut rig, &id), [4.0, 0.2, 1.0], 1e-3),
        "follows while held"
    );
    inject(&mut rig, vec![press(false)]);
    rig.step(1);
    inject(&mut rig, vec![down_at(-3.0, 6.0)]);
    rig.step(1);
    assert!(
        near(&target(&mut rig, &id), [4.0, 0.2, 1.0], 1e-3),
        "not once released"
    );
}

#[test]
fn click_actions_of_the_wrong_kind_warn_once() {
    let ring = Arc::new(LogRing::new(NonZeroUsize::new(1024).unwrap()));
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(ring.layer()))
        .unwrap();
    let mut rig = Rig::with_input(MAP, Some(ring));
    rig.floor();
    let swapped = hero(
        &mut rig,
        json!({"click": "aim", "pointer": "go", "hold": false, "layers": [], "max_distance": 1000.0}),
    );
    let unknown = hero(
        &mut rig,
        json!({"click": "jump", "pointer": "aim", "hold": false, "layers": [], "max_distance": 1000.0}),
    );
    rig.step(1);
    for _ in 0..3 {
        inject(&mut rig, vec![down_at(3.0, -4.0), press(true)]);
        rig.step(1);
        inject(&mut rig, vec![press(false)]);
        rig.step(1);
    }
    assert_eq!(target(&mut rig, &swapped), Value::Null);
    assert_eq!(target(&mut rig, &unknown), Value::Null);
    let records = rig.ok("log.tail", json!({"level": "warn", "limit": 1000}))["records"].clone();
    let about = |id: &str| {
        records
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r.to_string().contains(id) && r.to_string().contains("ClickToMove"))
            .count()
    };
    assert_eq!(about(&swapped), 1, "{records}");
    assert_eq!(about(&unknown), 1, "{records}");
}
