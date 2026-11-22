//! Navigation agents walking real Jolt characters (2.5b.1): arrival and determinism.
//!
//! The floor (`common::engine`) is walkable to ±9.5 at y = 0.2 with the default volume, whose
//! agent (radius 0.3, height 1.8) matches the default `CharacterController`. A character walking
//! from x = −5 to x = 5.23 at 3 m/s on flat ground moves 0.05 m a tick, so (as in `agents.rs`'
//! header) it has 10.23 − 0.05 t left on tick t and arrives on tick 195 at x = 4.75, with the
//! signal delivered on tick 196.

mod common;

use common::engine::{Rig, near};
use serde_json::{Value, json};

#[test]
fn nav_agent_arrives_within_tolerance_using_character_controller() {
    let mut rig = Rig::new();
    rig.floor();
    let hero = rig.character(
        -5.0,
        0.0,
        json!({"target": [5.23, 0.0, 0.0], "max_speed": 3.0}),
    );
    rig.step(1);
    let motion = rig.play(&hero, "CharacterMotion");
    assert!(near(&motion["velocity"], [3.0, 0.0, 0.0], 1e-4), "{motion}");
    rig.step(194); // ticks 1..=194
    assert_eq!(rig.play(&hero, "NavAgentState")["status"], "Moving");
    rig.step(1); // tick 195
    let state = rig.play(&hero, "NavAgentState");
    assert_eq!(state["status"], "Arrived", "{state}");
    let feet = rig.play(&hero, "Transform")["translation"].clone();
    assert!(near(&feet, [4.75, 0.0, 0.0], 1e-3), "{feet}");
    rig.step(60);
    let still = rig.play(&hero, "Transform")["translation"].clone();
    assert!(near(&still, [4.75, 0.0, 0.0], 1e-3), "it stops: {still}");
}

/// 16 characters on a 4 × 4 grid each walk to the opposite corner past two pillars, crowding
/// through the middle; at tick 120 every target turns a quarter. Returns the hashes along the way.
fn crowd(threads: usize) -> (Vec<String>, Vec<Value>) {
    let mut rig = Rig::with(threads, None);
    rig.floor();
    for x in [-2.0, 2.0] {
        rig.spawn(json!({
            "Transform": Rig::at(x, 1.0, 0.0),
            "RigidBody": {"kind": "Static"},
            "Collider": {"shape": {"Box": {"half_extents": [0.5, 1.0, 0.5]}}},
        }));
    }
    let spots = [-6.0f32, -2.0, 2.0, 6.0];
    let mut agents = Vec::new();
    for x in spots {
        for z in spots {
            let id = rig.character(x, z, json!({"target": [-x, 0.0, -z], "max_speed": 3.0}));
            agents.push((id, x, z));
        }
    }
    let mut hashes = vec![rig.step(1), rig.step(59), rig.step(60)];
    for (id, x, z) in &agents {
        rig.ok(
            "component.patch",
            json!({"entity": id, "component": "NavAgent", "world": "play",
                   "patch": {"target": [*z, 0.0, -*x]}}),
        );
    }
    hashes.push(rig.step(120));
    hashes.push(rig.step(240));
    let states = agents
        .iter()
        .map(|(id, _, _)| rig.play(id, "NavAgentState")["status"].clone())
        .collect();
    (hashes, states)
}

#[test]
fn nav_agents_are_deterministic_across_runs_and_threads() {
    let (one, states) = crowd(1);
    let (eight, _) = crowd(8);
    let (again, _) = crowd(8);
    assert_eq!(one, eight, "1 vs 8 threads");
    assert_eq!(eight, again, "run to run");
    assert!(
        states.iter().any(|s| s != "Idle"),
        "the agents moved: {states:?}"
    );
    let distinct: std::collections::BTreeSet<&String> = one.iter().collect();
    assert_eq!(
        distinct.len(),
        one.len(),
        "the world changed between hashes"
    );
}
