//! The Snuffer (M4 4.13a, N1–N3, N5): Lamplight's first enemy, on `scenes/harbour_watch.scene.ron`.
//! Its brain is the Behavior graph `ai/snuffer.graph.ron`; the rules drain the can when its `snuff`
//! lands.
//!
//! Timing by hand, at 60 Hz: the Snuffer's post is `POST`; its sight reaches 10 m in a 120° cone
//! facing +Z (toward the town). A lamplighter standing 5 m in front of it is seen on the first
//! sight pass, walked to at 2.5 m/s (arriving within 1.2 m in about 1.6 s), then snuffed; the
//! cooldown is 2 s, so ten seconds of standing still costs at least two snuffs (50 oil) and the
//! can never goes below 0. With no one in sight it walks back to its post (2.5 m/s; it stops
//! within its 1.2 m arrival distance).

use crate::lamplight_world as world;

use serde_json::{Value, json};
use world::Night;

/// The Snuffer's post on the harbour street (its `home`).
const POST: [f64; 3] = [0.0, 0.0, 26.0];

fn harbour() -> (Night, String) {
    let mut n = Night::in_scene("playing", "scenes/harbour_watch.scene.ron");
    let snuffer = n.named("Snuffer").0;
    (n, snuffer)
}

fn at(value: &Value) -> [f64; 3] {
    let t = &value["translation"];
    [0, 1, 2].map(|i| t[i].as_f64().unwrap())
}

fn flat_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[test]
fn the_snuffer_runs_on_its_behavior_graph() {
    let (mut n, snuffer) = harbour();
    let brain = n.named("Snuffer").1["ai.Brain"].clone();
    assert_eq!(brain["tree"], "ai/snuffer.graph.ron", "{brain}");
    n.step(2);
    // Play start compiled the graph: the brain has its blackboard (the graph's variables).
    let bb = n.play(&snuffer, "ai.Blackboard");
    for key in ["target", "target_position", "home"] {
        assert!(bb["keys"].get(key).is_some(), "{key} in {bb}");
    }
    assert_eq!(n.errors(), json!([]));
}

#[test]
fn a_snuffer_hunts_and_snuffs_the_lamplighter() {
    let (mut n, _) = harbour();
    let walker = n.place_lamplighter([POST[0], 0.0, POST[2] - 5.0]);
    let mut lowest = 100.0_f64;
    for _ in 0..10 {
        n.step(60);
        let can = n.play(&walker, "lamplight.Lamplighter")["can"]
            .as_f64()
            .unwrap();
        assert!(can >= 0.0, "the can never goes below 0: {can}");
        lowest = lowest.min(can);
    }
    assert!(
        lowest <= 50.0,
        "at least two snuffs in 10 s: the can holds {lowest}"
    );
    assert_eq!(n.errors(), json!([]));
}

/// With no one in sight (the lamplighter stays at the town's start, 10.8 m from where the Snuffer
/// begins: past its 10 m sight), the Snuffer walks back to its post. Forgetting a lost target is
/// the perception tests' job; walking away from it is the replay's (N6).
#[test]
fn a_snuffer_away_from_its_post_walks_back_to_it() {
    let (mut n, snuffer) = harbour();
    n.patch(
        &snuffer,
        "Transform",
        json!({"translation": [4.0, 0.0, 22.0]}),
    );
    n.step(10 * 60);
    let back = at(&n.play(&snuffer, "Transform"));
    assert!(
        flat_distance(back, POST) <= 1.2,
        "back at its post: {back:?}"
    );
    let walker = n.one("lamplight.Lamplighter", "play").0;
    let can = n.play(&walker, "lamplight.Lamplighter")["can"]
        .as_f64()
        .unwrap();
    assert_eq!(can, 100.0, "it never saw the lamplighter");
    assert_eq!(n.errors(), json!([]));
}

#[test]
fn the_snuffer_graph_is_canonical_and_compiles() {
    let path = concordia_tests::repo_root().join("examples/m2-lamplight/ai/snuffer.graph.ron");
    let text = std::fs::read_to_string(&path).unwrap();
    let graph = concordia_graph::read_graph(&text).unwrap();
    assert_eq!(
        concordia_graph::write_graph(&graph).unwrap(),
        text,
        "canonical text"
    );
    let tree =
        concordia_ai::compile_behavior(&graph, &concordia_graph::NodeLibrary::core()).unwrap();
    assert_eq!(tree.keys.len(), 3, "{:?}", tree.keys);
}

/// The cast node of `ai/snuffer.graph.ron`.
const CAST_NODE: &str = "0193c4a0-5e1f-7000-8000-000000000008";

/// N9: an agent edits the Snuffer's graph over the Engine API (what MCP's `graph_apply` calls);
/// the edit is attributed in the command log and is what plays (a cast renamed away from `snuff`
/// takes no oil); undoing it restores the file byte for byte.
#[test]
fn graph_authored_over_mcp_is_used_in_play() {
    let (mut n, _) = harbour();
    n.place_lamplighter([POST[0], 0.0, POST[2] - 5.0]);
    let path = n.root().join("ai/snuffer.graph.ron");
    let before = std::fs::read(&path).unwrap();
    let applied = n.ok(
        "graph.apply",
        json!({"path": "ai/snuffer.graph.ron", "actor": "agent:gameplay-programmer#1",
            "edits": [{"SetValue": {"node": CAST_NODE, "pin": "skill", "value": {"Text": "whisper"}}}]}),
    );
    assert_eq!(applied["ok"], true, "{applied}");
    let history = n.ok("txn.history", json!({}));
    let last = history["entries"]
        .as_array()
        .and_then(|e| e.last())
        .cloned()
        .unwrap_or_default();
    assert_eq!(last["actor"], "agent:gameplay-programmer#1", "{history}");

    n.step(10 * 60);
    let walker = n.one("lamplight.Lamplighter", "play").0;
    let can = n.play(&walker, "lamplight.Lamplighter")["can"]
        .as_f64()
        .unwrap();
    assert_eq!(can, 100.0, "the edited graph plays: a whisper takes no oil");

    n.ok("sim.stop", json!({}));
    n.ok("txn.undo", json!({"actor": "agent:gameplay-programmer#1"}));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "undo restores the graph byte for byte"
    );
}

/// The committed replay (N6): 40 s on the harbour street. The lamplighter walks south into the
/// Snuffer's sight (S held 1.6 s), stands while it closes and snuffs, then walks east (D held
/// 10 s), faster than it; it loses them about 20 s in and is back at its post by about 29 s.
const REPLAY: &str = "tests/replays/m4_snuffer.replay.ron";
pub const REPLAY_TICKS: u32 = 2400;
pub const SCRIPT: [(u64, &str, bool); 4] = [
    (0, "S", true),
    (96, "S", false),
    (576, "D", true),
    (1176, "D", false),
];

/// The playtest assertions over the replay (N6). A snuff is raised on the tick after the cast, so
/// "within reach" allows the Snuffer's step since (1.6 m + 0.2).
pub const SNUFFER_ASSERTIONS: [&str; 5] = [
    r#"within 600 ticks: event("lamplight.Snuffed")"#,
    r#"at 660 ticks: name("Lamplighter").lamplight.Lamplighter.can <= 50"#,
    r#"never: name("Lamplighter").lamplight.Lamplighter.can < 0"#,
    r#"never: event("lamplight.Snuffed") and distance(name("Snuffer"), name("Lamplighter")) > 1.8m"#,
    r#"after 1200 ticks, within 1200 ticks: name("Snuffer").Transform.translation.z >= 24.8 and name("Snuffer").Transform.translation.x <= 1.2 and name("Snuffer").Transform.translation.x >= -1.2"#,
];

/// The harbour with the committed replay copied into the project.
fn with_committed() -> Night {
    let (n, _) = harbour();
    let to = n.root().join(REPLAY);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::copy(concordia_tests::repo_root().join(REPLAY), to).unwrap();
    n
}

#[test]
fn the_committed_snuffer_replay_verifies_on_its_platform() {
    let mut n = with_committed();
    let text = std::fs::read_to_string(n.root().join(REPLAY)).unwrap();
    let here = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let run = match text.contains(&here) {
        true => n.ok("replay.run", json!({"path": REPLAY})),
        false => n.ok(
            "replay.run",
            json!({"path": REPLAY, "mode": "resimulate", "force": true}),
        ),
    };
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["ticks"], REPLAY_TICKS);
}

#[test]
fn the_snuffer_replay_assertions_pass() {
    let mut n = with_committed();
    let checked = n.ok(
        "assert.eval",
        json!({"source": "replay", "path": REPLAY, "assertions": SNUFFER_ASSERTIONS}),
    );
    assert_eq!(checked["ok"], true, "{checked:#}");
}

/// Records the committed replay anew and writes it into the repository. Run by hand after an
/// intended change to the Snuffer, its scene or the rules.
#[test]
#[ignore = "rewrites tests/replays/m4_snuffer.replay.ron; run by hand to re-bless"]
fn rebless_the_committed_snuffer_replay() {
    let (mut n, _) = harbour();
    n.ok(
        "replay.record",
        json!({"path": REPLAY, "checkpoint_every": 600, "stills": false}),
    );
    n.keys(&SCRIPT);
    n.step(REPLAY_TICKS);
    n.ok("replay.record", json!({"action": "stop"}));
    let text = std::fs::read_to_string(n.root().join(REPLAY)).unwrap();
    std::fs::write(concordia_tests::repo_root().join(REPLAY), text).unwrap();
}
