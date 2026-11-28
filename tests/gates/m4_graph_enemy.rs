//! The M4 gate, the graph-scripted enemy (M4 plan, "Milestone gate" 4; 4.13d, refinement R1 and
//! N1–N10): Lamplight's Snuffer, whose brain is the Behavior graph `ai/snuffer.graph.ron` compiled
//! by `concordia-graph`, run headless through the Engine API. Its committed replay verifies tick for
//! tick and its playtest assertions hold; a hand-written Rust twin leaves the world identical on
//! every tick; and an agent's graph edit over the API is attributed, plays, and undoes byte for
//! byte. The scene, rules and helpers live in `lamplight_snuffer` and `snuffer_twin`.

use std::collections::BTreeSet;

use serde_json::json;

use crate::lamplight_snuffer::REPLAY_TICKS;
use crate::lamplight_snuffer::{
    CAST_NODE, POST, REPLAY, SNUFFER_ASSERTIONS, harbour, with_committed,
};
use crate::snuffer_twin::{night, parts};

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

#[test]
fn snuffer_graph_replay_verifies_tick_for_tick() {
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
fn snuffer_graph_replay_assertions_pass() {
    let mut n = with_committed();
    let checked = n.ok(
        "assert.eval",
        json!({"source": "replay", "path": REPLAY, "assertions": SNUFFER_ASSERTIONS}),
    );
    assert_eq!(checked["ok"], true, "{checked:#}");
}

#[test]
fn graph_brain_matches_rust_brain_per_tick() {
    let (mut graph, mut rust) = (night(false), night(true));
    for tick in 1..=crate::lamplight_snuffer::REPLAY_TICKS {
        graph.step(1);
        rust.step(1);
        let (g, r) = (parts(&graph), parts(&rust));
        let differ: BTreeSet<&String> = g
            .keys()
            .chain(r.keys())
            .filter(|k| g.get(*k) != r.get(*k))
            .collect();
        assert!(differ.is_empty(), "tick {tick}: {differ:?} differ");
    }
    // Both brains really acted: the lamplighter was snuffed.
    let walker = graph.one("lamplight.Lamplighter", "play").0;
    let can = graph.play(&walker, "lamplight.Lamplighter")["can"]
        .as_f64()
        .unwrap();
    assert!(can <= 50.0, "{can}");
}
