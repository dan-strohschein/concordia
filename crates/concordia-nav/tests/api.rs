//! The nav engine module: `nav.build` and `nav.path` over the Engine API, edits, the play world
//! and the `navmesh` debug layer.
//!
//! The fixture floor has half 10.1 (edges inside a cell) and the default volume (0.25 m cells,
//! radius 0.3 m = 2 cells, 64-cell tiles of 16 m): the ledge cell and two more are removed, so it
//! is walkable to ±9.5 at the surface y = −3 + 16 × 0.2 = 0.2. The 24 m volume is 2 × 2 tiles,
//! with seams at x = 4 and z = 4.

mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;

use common::engine::{Rig, near};
use concordia_api::{ApiErrorKind, WorldSel};
use concordia_core::log::LogRing;
use concordia_nav::NavMeshes;
use serde_json::json;
use tracing_subscriber::layer::SubscriberExt;

#[test]
fn nav_build_over_api_reports_tiles_polygons_and_islands() {
    let mut rig = Rig::new();
    let e = rig.call("nav.build", json!({})).unwrap_err();
    assert_eq!(e.kind, ApiErrorKind::EntityNotFound, "{e:?}");
    assert!(e.message.contains("NavMeshVolume"), "{e:?}");
    assert!(e.data.suggestions[0].contains("NavMeshVolume"), "{e:?}");
    let volume = rig.floor();
    let first = rig.ok("nav.build", json!({}));
    let meshes = first["navmeshes"].as_array().unwrap();
    assert_eq!(meshes.len(), 1, "{first}");
    let m = &meshes[0];
    assert_eq!(m["volume"], json!(volume));
    assert_eq!(m["tiles"], 4);
    assert!(m["polygons"].as_u64().unwrap() >= 4, "{m}");
    assert_eq!(m["islands"], 1);
    assert_eq!(m["cached"], false);
    assert_eq!(m["key"].as_str().unwrap().len(), 64);
    assert!(near(&m["min"], [-9.5, 0.2, -9.5], 1e-3), "{m}");
    assert!(near(&m["max"], [9.5, 0.2, 9.5], 1e-3), "{m}");
    let second = rig.ok("nav.build", json!({"volume": volume}));
    assert_eq!(second["navmeshes"][0]["cached"], true, "{second}");
    assert_eq!(second["navmeshes"][0]["key"], m["key"]);
    let bad = rig.spawn(json!({
        "Transform": Rig::at(50.0, 0.0, 0.0),
        "NavMeshVolume": {"half_extents": [5.0, 0.0, 5.0]},
    }));
    let e = rig.call("nav.build", json!({"volume": bad})).unwrap_err();
    assert_eq!(e.kind, ApiErrorKind::ValueInvalid, "{e:?}");
    assert!(e.message.contains("half_extents"), "{e:?}");
}

#[test]
fn nav_path_over_api_returns_waypoints() {
    let mut rig = Rig::new();
    rig.floor();
    // Across the x = 4 seam on open floor: one segment, √(13² + 1²) long.
    let path = rig.ok(
        "nav.path",
        json!({"from": [-6.0, 0.0, 1.0], "to": [7.0, 0.0, 2.0]}),
    );
    assert_eq!(path["partial"], false, "{path}");
    let points = path["waypoints"].as_array().unwrap();
    assert_eq!(points.len(), 2, "{path}");
    assert!(
        near(&points[0], [-6.0, 0.2, 1.0], 1e-3) && near(&points[1], [7.0, 0.2, 2.0], 1e-3),
        "{path}"
    );
    assert!(
        (path["length"].as_f64().unwrap() - 170f64.sqrt()).abs() < 1e-3,
        "{path}"
    );
    // A wall across x ∈ [−0.5, 0.5], z ≤ 6 (a static box) is seen without a nav.build call.
    rig.spawn(json!({
        "Transform": Rig::at(0.0, 1.0, -2.0),
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [0.5, 1.0, 8.0]}}},
    }));
    let around = rig.ok(
        "nav.path",
        json!({"from": [-6.0, 0.0, 1.0], "to": [7.0, 0.0, 2.0]}),
    );
    assert!(
        around["waypoints"].as_array().unwrap().len() > 2,
        "{around}"
    );
    assert!(around["length"].as_f64().unwrap() > 13.5, "{around}");
    let off = rig
        .call(
            "nav.path",
            json!({"from": [80.0, 0.0, 0.0], "to": [0.0, 0.0, 0.0]}),
        )
        .unwrap_err();
    assert_eq!(off.kind, ApiErrorKind::ValueInvalid, "{off:?}");
    assert!(off.message.contains("navmesh"), "{off:?}");
    // The play world gets its own navmeshes, as a resource processors can read.
    rig.ok("sim.step", json!({"ticks": 1}));
    let play = rig.ok(
        "nav.path",
        json!({"from": [-6.0, 0.0, 1.0], "to": [7.0, 0.0, 2.0], "world": "play"}),
    );
    assert_eq!(play, around);
    let world = rig.engine.world(WorldSel::play).unwrap();
    let meshes = world
        .resource::<NavMeshes>()
        .expect("the play world has its navmeshes");
    assert_eq!(meshes.len(), 1);
}

#[test]
fn navmesh_debug_layer_draws_every_polygon() {
    let mut rig = Rig::new();
    assert!(rig.engine.debug_layers().contains(&"navmesh"));
    assert_eq!(
        rig.engine.debug_geometry("navmesh", WorldSel::edit),
        None,
        "nothing to draw yet"
    );
    rig.floor();
    let built = rig.ok("nav.build", json!({}));
    let polygons = built["navmeshes"][0]["polygons"].as_u64().unwrap() as usize;
    let geometry = rig
        .engine
        .debug_geometry("navmesh", WorldSel::edit)
        .expect("drawn");
    // Each polygon is a fan of (n − 2) triangles and an outline of n edges, lifted 2 cm.
    let sides: usize = geometry.lines.len();
    assert!(
        geometry.triangles.len() >= polygons && sides >= 3 * polygons,
        "{polygons} polygons"
    );
    assert_eq!(
        sides - geometry.triangles.len(),
        2 * polygons,
        "n edges and n − 2 triangles each"
    );
    for t in &geometry.triangles {
        assert!(t.points.iter().all(|p| (p.y - 0.22).abs() < 1e-3), "{t:?}");
    }
    assert_eq!(
        rig.engine.debug_geometry("no-such-layer", WorldSel::edit),
        None
    );
}

#[test]
fn nav_agent_moves_in_the_play_world_over_the_api() {
    // From (−5, 0) to (5.23, 0) at 3 m/s, as in `agents_physics.rs`: the target is set on tick 1,
    // so the walk starts there and arrives on tick 196.
    let mut rig = Rig::new();
    rig.floor();
    let hero = rig.character(-5.0, 0.0, json!({"max_speed": 3.0}));
    rig.step(1);
    assert_eq!(rig.play(&hero, "NavAgentState")["status"], "Idle");
    rig.ok(
        "component.patch",
        json!({"entity": hero, "component": "NavAgent", "world": "play",
               "patch": {"target": [5.23, 0.0, 0.0]}}),
    );
    rig.step(1);
    let state = rig.play(&hero, "NavAgentState");
    assert_eq!(state["status"], "Moving", "{state}");
    assert!(
        near(&state["waypoints"][1], [5.23, 0.2, 0.0], 1e-3),
        "{state}"
    );
    rig.step(195);
    assert_eq!(rig.play(&hero, "NavAgentState")["status"], "Arrived");
    let edit = rig.ok("entity.get", json!({"id": hero}));
    assert!(
        edit["components"].get("NavAgentState").is_none(),
        "play never writes the edit world"
    );
}

#[test]
fn nav_agent_size_mismatch_warns_once() {
    let ring = Arc::new(LogRing::new(NonZeroUsize::new(1024).unwrap()));
    // Processors may log from job threads, so the ring is the process's subscriber.
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(ring.layer()))
        .unwrap();
    let mut rig = Rig::with(2, Some(ring));
    rig.floor();
    let wide = rig.character(-5.0, 0.0, json!({"target": [5.0, 0.0, 0.0], "radius": 0.5}));
    rig.character(-5.0, 3.0, json!({"target": [5.0, 0.0, 3.0]}));
    rig.step(5);
    rig.ok(
        "component.patch",
        json!({"entity": wide, "component": "NavAgent", "world": "play",
               "patch": {"target": [5.0, 0.0, -3.0]}}),
    );
    rig.step(5);
    let records = rig.ok("log.tail", json!({"level": "warn", "limit": 1000}))["records"].clone();
    let count = |phrase: &str| {
        records
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["message"].as_str().is_some_and(|m| m.contains(phrase)))
            .count()
    };
    assert_eq!(
        count("differs from its CharacterController"),
        1,
        "{records}"
    );
    assert_eq!(count("wider than its navmesh"), 1, "{records}");
    assert_eq!(
        rig.play(&wide, "NavAgentState")["status"],
        "Moving",
        "it still moves"
    );
}

#[test]
fn nav_paths_debug_layer_draws_remaining_paths() {
    // A wall across x ∈ [−0.5, 0.5], z ≤ 6 makes the path bend; the layer draws the feet to the
    // next corner, then each corner to the next: (waypoints − next) lines, lifted 2 cm.
    let mut rig = Rig::new();
    assert!(rig.engine.debug_layers().contains(&"nav_paths"));
    rig.floor();
    rig.spawn(json!({
        "Transform": Rig::at(0.0, 1.0, -2.0),
        "RigidBody": {"kind": "Static"},
        "Collider": {"shape": {"Box": {"half_extents": [0.5, 1.0, 8.0]}}},
    }));
    let hero = rig.character(-6.0, 1.0, json!({"target": [7.0, 0.0, 2.0]}));
    rig.character(3.0, -8.0, json!({}));
    // A crowd agent walking the same way is the `crowd` layer's, not this one's.
    rig.spawn(json!({
        "Transform": Rig::at(-6.0, 0.0, 3.0),
        "NavAgent": {"target": [7.0, 0.0, 4.0]},
        "CrowdAgent": {},
    }));
    assert_eq!(
        rig.engine.debug_geometry("nav_paths", WorldSel::edit),
        None,
        "edit worlds don't walk"
    );
    rig.step(1);
    let state = rig.play(&hero, "NavAgentState");
    let waypoints = state["waypoints"].as_array().unwrap().clone();
    let next = state["next"].as_u64().unwrap() as usize;
    assert!(waypoints.len() > 2, "{state}");
    let geometry = rig
        .engine
        .debug_geometry("nav_paths", WorldSel::play)
        .expect("drawn");
    assert_eq!(geometry.lines.len(), waypoints.len() - next, "{geometry:?}");
    assert!(geometry.triangles.is_empty());
    let feet = rig.play(&hero, "Transform")["translation"].clone();
    let first = geometry.lines[0];
    let lifted = |v: &serde_json::Value| {
        [
            v[0].as_f64().unwrap() as f32,
            v[1].as_f64().unwrap() as f32 + 0.02,
            v[2].as_f64().unwrap() as f32,
        ]
    };
    assert_eq!(first.from.to_array(), lifted(&feet), "{first:?}");
    assert_eq!(first.to.to_array(), lifted(&waypoints[next]), "{first:?}");
    let last = geometry.lines.last().unwrap();
    assert_eq!(last.to.to_array(), lifted(waypoints.last().unwrap()));
}
