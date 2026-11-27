//! Lamplight's town life in the engine (M2 phase 2.10c.2): townsfolk, wagons and refills.
//!
//! By hand, at 60 Hz (a signal raised on tick t is delivered on t + 1; the lamplighter starts at
//! (0, 0, 12)):
//! - Townsperson 13 lives by the east row's last door, at (19.5, 0, 7.1) (residents stand
//!   0.9 m apart along the wall), and shops at stall 1, 5.5 m out at 135°: (−3.89, 0, 3.89),
//!   23.6 m away. 19.9 m from the lamplighter is LOD level
//!   0, so they update every tick. After 2 s strolling at up to 1.2 m/s (accelerating from rest)
//!   they are at least 1.5 m closer to the market, aimed at it, with a walk blend above 0.5.
//! - A wisp 3 m from them (fear reaches 5 m) is broadcast at the end of tick 0, heard on tick
//!   1: they are frightened, aimed home at 3 m/s.
//! - LOD: with the lamplighter at (38, 0, 38), townsperson 0 (door (−15, 0, −19.5)) is 78 m
//!   away, past 70 m (level 3, every 8 ticks), and townsperson 13 is 35 m away (25–45 m, level 1,
//!   every 2 ticks). Over 80 ticks they update about 10 and 40 times (±2: the first tick is
//!   always due, and the stagger shifts where the period starts).
//! - Wagons: four drive the ring road, one way, 3.5 m long, at up to 4 m/s. No lane or junction
//!   has a problem, and over 20 s their centres never come within a wagon's length.
//! - Refill: wagon 1 starts on the east side at (16, 0, 0), heading north. The lamplighter
//!   stands in the lane 8 m ahead at (16, 0, −8) with 20 oil. The wagon slows to walking pace about
//!   1.75 (half its length) + 1.5 (its gap) + the lamplighter's radius from them, within the 4.5 m
//!   refill reach, and fills the can: 100 oil, one refill.

use crate::lamplight_world as world;

use serde_json::{Value, json};
use world::{Night, near};

fn xz(v: &Value) -> (f64, f64) {
    (v[0].as_f64().unwrap(), v[2].as_f64().unwrap())
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// A townsperson's walker and body.
fn townsperson(n: &mut Night, index: u32) -> (String, String) {
    let (walker, c) = n.named(&format!("Townsperson {index}"));
    let body = c["fog.Townsperson"]["body"].as_str().unwrap().to_owned();
    (walker, body)
}

#[test]
fn townsfolk_stroll_to_the_market() {
    let mut n = Night::new("title");
    let (walker, body) = townsperson(&mut n, 13);
    let market = (-3.89, 3.89);
    n.step(120);
    let at = xz(&n.play(&walker, "Transform")["translation"]);
    assert!(
        distance(at, market) <= 23.6 - 1.5,
        "walked toward the market: {at:?}"
    );
    let target = n.play(&walker, "NavAgent")["target"].clone();
    assert!(
        distance(xz(&target), market) < 0.01,
        "aimed at the market: {target}"
    );
    let blend = n.play(&body, "AnimGraphPlayer")["floats"]["walk_amount"]
        .as_f64()
        .unwrap();
    assert!(blend > 0.5, "walking: {blend}");
    assert_eq!(n.errors(), json!([]));
}

#[test]
fn a_wisp_close_by_frightens_a_townsperson_home() {
    let mut n = Night::new("title");
    let (walker, _) = townsperson(&mut n, 13);
    let wisp = n.wisp_at([16.5, 0.0, 7.1]);
    n.patch(&wisp, "fog.Drifter", json!({"target": [16.5, 0.0, 7.1]}));
    n.step(3);
    let person = n.play(&walker, "fog.Townsperson");
    assert_eq!(person["scared"], true, "{person}");
    let agent = n.play(&walker, "NavAgent");
    assert!(
        distance(xz(&agent["target"]), (19.5, 7.1)) < 0.01,
        "home: {agent}"
    );
    assert_eq!(agent["max_speed"], 3.0, "{agent}");
}

#[test]
fn far_townsfolk_update_at_their_lod_rate() {
    let mut n = Night::new("title");
    n.place_lamplighter([38.0, 0.0, 38.0]);
    let (far, _) = townsperson(&mut n, 0);
    let (near_one, _) = townsperson(&mut n, 13);
    n.step(80);
    let far_updates = n.play(&far, "fog.Townsperson")["updates"].as_f64().unwrap();
    let near_updates = n.play(&near_one, "fog.Townsperson")["updates"]
        .as_f64()
        .unwrap();
    assert!(
        near(far_updates, 10.0, 2.0),
        "level 3 updates every 8 ticks: {far_updates}"
    );
    assert!(
        near(near_updates, 40.0, 2.0),
        "level 1 every 2: {near_updates}"
    );
}

#[test]
fn wagons_drive_the_ring_road_without_meeting() {
    let mut n = Night::new("title");
    for (id, c) in n.all("Lane", "edit") {
        assert!(c.get("TrafficProblems").is_none(), "lane {id}: {c}");
    }
    let wagons: Vec<String> = n
        .all("lamplight.Wagon", "edit")
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(wagons.len(), 4);
    let mut closest = f64::MAX;
    for _ in 0..60 {
        n.step(20);
        let at: Vec<(f64, f64)> = wagons
            .iter()
            .map(|w| xz(&n.play(w, "Transform")["translation"]))
            .collect();
        for i in 0..at.len() {
            for j in i + 1..at.len() {
                closest = closest.min(distance(at[i], at[j]));
            }
        }
    }
    assert!(closest >= 3.5, "wagons met: {closest}");
    for w in &wagons {
        let state = n.play(w, "VehicleState");
        assert!(state["lane"].is_string(), "on a lane: {state}");
        let problems = n.all("TrafficProblems", "play");
        assert!(problems.is_empty(), "{problems:?}");
    }
    assert_eq!(n.errors(), json!([]));
}

#[test]
fn a_wagon_brakes_for_the_lamplighter_and_refills_the_can() {
    let mut n = Night::new("playing");
    let fog = n.one("fog.Fog", "edit").0;
    n.patch(&fog, "fog.Fog", json!({"next": 1000.0}));
    let walker = n.place_lamplighter([16.0, 0.0, -8.0]);
    n.patch(&walker, "lamplight.Lamplighter", json!({"can": 20.0}));
    let (wagon, _) = n.named("Wagon 1");
    n.step(600);
    let state = n.play(&wagon, "VehicleState");
    assert!(state["speed"].as_f64().unwrap() <= 0.5, "stopped: {state}");
    let gap = distance(
        xz(&n.play(&wagon, "Transform")["translation"]),
        (16.0, -8.0),
    );
    assert!(gap <= 4.5, "stopped within reach: {gap}");
    assert_eq!(n.play(&walker, "lamplight.Lamplighter")["can"], 100.0);
    assert_eq!(n.play(&wagon, "lamplight.Wagon")["refills"], 1);
    assert_eq!(n.errors(), json!([]));
}
