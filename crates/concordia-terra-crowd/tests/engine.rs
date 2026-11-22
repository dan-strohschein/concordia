//! The crowd in an engine (2.5c.1): physics agents with Jolt, click-to-move, the `Crowd` template
//! trait and the `crowd` debug layer, over the Engine API.
//!
//! The floor (`common::engine`) has its top at y = 0 and is walkable to ±9.5 at y = 0.2 with the
//! default volume (a 0.3 m agent radius). Default agents and controllers are 0.3 m wide.

mod common;

use std::sync::Arc;

use common::engine::{Engined, near};
use concordia_api::WorldSel;
use serde_json::{Value, json};

fn rig() -> Engined {
    Engined::new(2, Vec::new(), |_| {})
}

fn xyz(v: &Value) -> [f32; 3] {
    [0, 1, 2].map(|i| v[i].as_f64().unwrap() as f32)
}

fn gap(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// A physics crowd agent: a controller, its motion, a `NavAgent` going to `to` and a `CrowdAgent`.
fn physics_agent(rig: &mut Engined, x: f32, z: f32, to: [f32; 3]) -> String {
    rig.spawn(json!({
        "Transform": Engined::at(x, 0.0, z),
        "CharacterController": {},
        "CharacterMotion": {},
        "NavAgent": {"target": to, "max_speed": 3.0},
        "CrowdAgent": {"avoidance": null},
    }))
}

#[test]
fn physics_crowd_agents_keep_apart_and_arrive() {
    // Two Jolt characters walk head-on, 0.2 m apart sideways (under 2 × 0.3, so they meet). Jolt
    // characters don't collide with each other, so only the crowd keeps them apart: its push
    // goes to `CharacterMotion` as a velocity, and Jolt's flat-ground step follows it (2.5b.1:
    // within 1 mm). They close at 6 m/s at most, so (`crowd.rs`'s header) the overlap stays under
    // 0.0081 × 0.1 / 0.9919 = 0.000817 m, plus Jolt's 1 mm. Both arrive.
    let mut rig = rig();
    rig.floor();
    let a = physics_agent(&mut rig, -3.0, 0.0, [3.0, 0.0, 0.0]);
    let b = physics_agent(&mut rig, 3.0, 0.2, [-3.0, 0.0, 0.2]);
    rig.step(1);
    let mut deepest = 0.0f32;
    let mut done = false;
    for _ in 0..300 {
        rig.step(1);
        let pa = xyz(&rig.play(&a, "Transform")["translation"]);
        let pb = xyz(&rig.play(&b, "Transform")["translation"]);
        let overlap = 0.6 - gap(pa, pb);
        deepest = deepest.max(overlap);
        assert!(overlap <= 0.000817 + 1e-3, "overlap {overlap}");
        let arrived =
            |rig: &mut Engined, id: &str| rig.play(id, "NavAgentState")["status"] == "Arrived";
        if arrived(&mut rig, &a) && arrived(&mut rig, &b) {
            done = true;
            break;
        }
    }
    assert!(done, "both arrive");
    assert!(deepest > 0.0, "they did meet");
    let motion = rig.play(&a, "CharacterMotion");
    assert!(
        near(&motion["velocity"], [0.0, 0.0, 0.0], 1e-6),
        "stopped: {motion}"
    );
}

const MAP: &str = r#"InputMap(
    actions: {
        "aim": (kind: Ray, bindings: [PointerRay]),
        "go": (kind: Button, bindings: [MouseButton(Left)]),
    },
)"#;

#[test]
fn click_to_move_drives_a_crowd_agent() {
    // ClickToMove writes `NavAgent.target` and needs no controller, so it drives a surface crowd
    // agent unchanged: a ray straight down at (3, −4) hits the floor's top and snaps up to the
    // surface (3, 0.2, −4) (nav's click tests). Physics sees bodies from tick 1.
    let input = concordia_input::InputModule::new(concordia_input::InputMap::parse(MAP).unwrap());
    let mut rig = Engined::new(2, vec![Arc::new(input)], |_| {});
    rig.floor();
    let id = rig.spawn(json!({
        "Transform": Engined::at(-5.0, 0.0, 0.0),
        "NavAgent": {"max_speed": 3.0},
        "CrowdAgent": {},
        "ClickToMove": {"click": "go", "pointer": "aim", "hold": false, "layers": [], "max_distance": 1000.0},
    }));
    rig.step(1);
    let events = [
        json!({"PointerRay": {"origin": [3.0, 10.0, -4.0], "direction": [0.0, -1.0, 0.0]}}),
        json!({"MouseButton": {"button": "Left", "pressed": true}}),
    ];
    let events: Vec<Value> = events.into_iter().map(|e| json!({"event": e})).collect();
    rig.ok("input.inject", json!({ "events": events }));
    rig.step(1);
    let target = rig.play(&id, "NavAgent")["target"].clone();
    assert!(near(&target, [3.0, 0.2, -4.0], 1e-3), "{target}");
    rig.step(240);
    assert_eq!(rig.play(&id, "NavAgentState")["status"], "Arrived");
    let feet = xyz(&rig.play(&id, "Transform")["translation"]);
    assert!(gap(feet, [3.0, 0.2, -4.0]) <= 0.5, "{feet:?}");
}

const GRUNT: &str = r#"Template(
    format: 1,
    description: "A grunt: walks with the crowd.",
    traits: {
        "Crowd": (agent: (max_speed: 3.5), crowd: (separation: true)),
    },
    components: {
        "Name": "Grunt",
    },
)
"#;

#[test]
fn crowd_template_spawns_a_thousand_agents() {
    // A spawner of 1,000 grunts on a 0.6 m grid: each gets the trait's `NavAgent` (3.5 m/s, no
    // target), its `CrowdAgent` (separation on) and, at play start, a `CrowdState`.
    let mut rig = Engined::new(2, Vec::new(), |root| {
        std::fs::create_dir_all(root.join("templates")).unwrap();
        std::fs::write(root.join("templates/grunt.template.ron"), GRUNT).unwrap();
    });
    rig.floor();
    let listed = rig.ok("entity.templates", json!({}));
    let grunt = listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "grunt")
        .unwrap_or_else(|| panic!("{listed}"))
        .clone();
    assert_eq!(grunt["traits"], json!(["Crowd"]), "{grunt}");
    assert_eq!(grunt["problem"], Value::Null, "{grunt}");
    rig.spawn(json!({
        "Transform": Engined::at(0.0, 0.0, 0.0),
        "Spawner": {"template": "grunt", "count": 1000, "seed": 7,
                    "distribution": {"Grid": {"spacing": 0.6}}},
    }));
    rig.step(1);
    let rows = rig.ok(
        "query.run",
        json!({"with": ["NavAgent", "CrowdAgent", "CrowdState", "Name"], "world": "play", "limit": 1000}),
    )["rows"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(rows.len(), 1000);
    let id = rows[0]["id"].as_str().unwrap().to_owned();
    let agent = rig.play(&id, "NavAgent");
    assert_eq!(
        (agent["max_speed"].as_f64(), agent["target"].clone()),
        (Some(3.5), Value::Null)
    );
    assert_eq!(rig.play(&id, "CrowdAgent")["separation"], true);
}

#[test]
fn crowd_debug_layer_draws_agents_velocities_and_links() {
    // Two surface agents 1 m apart, walking +x; a third far away has no neighbours. The layer
    // draws per agent a 16-sided ring (16 lines), its velocity and desired velocity (as 0.5 s of
    // travel, when not zero), its corners (feet → corner → …), and one link per neighbour pair,
    // all lifted 2 cm.
    let mut rig = rig();
    assert!(rig.engine.debug_layers().contains(&"crowd"));
    rig.floor();
    let a = rig.spawn(json!({
        "Transform": Engined::at(-5.0, 0.0, 0.0),
        "NavAgent": {"target": [5.0, 0.0, 0.0], "max_speed": 3.0},
        "CrowdAgent": {},
    }));
    rig.spawn(json!({
        "Transform": Engined::at(-5.0, 0.0, 1.0),
        "NavAgent": {"target": [5.0, 0.0, 1.0], "max_speed": 3.0},
        "CrowdAgent": {},
    }));
    rig.spawn(json!({
        "Transform": Engined::at(5.0, 0.0, -8.0),
        "NavAgent": {},
        "CrowdAgent": {},
    }));
    assert_eq!(
        rig.engine.debug_geometry("crowd", WorldSel::edit),
        None,
        "edit worlds don't walk"
    );
    rig.step(2);
    let geometry = rig
        .engine
        .debug_geometry("crowd", WorldSel::play)
        .expect("drawn");
    // Rings 3 × 16; the two walkers' velocity, desired velocity and one corner line each (2 × 3);
    // one link between the walkers (the far one has no neighbours).
    assert_eq!(geometry.lines.len(), 48 + 6 + 1, "{geometry:?}");
    assert!(geometry.triangles.is_empty());
    let feet = xyz(&rig.play(&a, "Transform")["translation"]);
    let state = rig.play(&a, "CrowdState");
    let v = xyz(&state["velocity"]);
    let lifted = |p: [f32; 3]| [p[0], p[1] + 0.02, p[2]];
    let velocity_line = geometry
        .lines
        .iter()
        .find(|l| {
            l.from.to_array() == lifted(feet) && (l.to.x - (feet[0] + v[0] * 0.5)).abs() < 1e-5
        })
        .unwrap_or_else(|| panic!("a velocity line from the feet: {geometry:?}"));
    assert!((velocity_line.to.z - (feet[2] + v[2] * 0.5)).abs() < 1e-5);
}
