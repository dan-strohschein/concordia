//! M3 gate: the seeded bug, its fix, and the replays and assertions that tell them apart, over
//! MCP stdio on copies of `tests/fixtures/m3-studio-game` (Studio off, so the gate drives the
//! engine directly).
//!
//! By hand (60 Hz; the player module moves the player each tick by the `move` axis times
//! `step(axis, speed, dt)`, before physics; W is held from tick 0, so the axis is (0, 1) from the
//! first tick; speed 5.5 m/s; the door is at z = 10):
//! - **The seeded bug** (`speed * dt * dt`): 5.5 / 3600 m a tick, so after 600 ticks the player
//!   is at z = 600 × 5.5 / 3600 = 0.9167 m and 9.0833 m from the door. "Within 600 ticks the
//!   player is within 2 m" fails at tick 600, closest at tick 600, 9.0833 m.
//! - **The fix** (`speed * dt`): 5.5 / 60 m a tick, so after t ticks z = 0.091667 t. The door is
//!   under 2 m away once z > 8, at t = 88 (z = 8.0667, 1.9333 m; at 87, z = 7.975). It passes at
//!   tick 88.
//! - **A/B:** both builds agree until the first moved tick: the bug and the fix first differ
//!   after tick 0's step (tick 1), in `Transform`, and `player.walk` is the suspect.

use std::path::Path;

use concordia_tests::bins::concordia_mcp_bin;
use concordia_tests::m3_fixture;
use concordia_tests::mcp_client::McpClient;
use serde_json::{Value, json};

const REACH: &str = r#"within 600 ticks: distance(tag("player"), tag("door")) < 2m"#;
const PLAYTEST: &str = "tests/playtests/walk-to-door.playtest.ron";
const REPLAY: &str = "tests/replays/walk.replay.ron";

/// A built copy of the fixture with Studio off; `fixed` applies `fix.patch` first.
fn game(dir: &Path, fixed: bool) -> std::path::PathBuf {
    let root = m3_fixture::copy(dir);
    if fixed {
        m3_fixture::apply_fix(&root);
    }
    m3_fixture::build(&root);
    let manifest = root.join("concordia.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, text.replace("\n[studio]\n", "\n")).unwrap();
    root
}

async fn server(root: &Path, threads: usize) -> McpClient {
    let threads = threads.to_string();
    let root = root.to_str().unwrap();
    let args = [
        "--project",
        root,
        "--actor",
        "agent:gate",
        "--deterministic-ids",
        "3",
        "--threads",
        &threads,
    ];
    let client = McpClient::spawn(&concordia_mcp_bin(), &args)
        .await
        .expect("server starts");
    ok(
        &client,
        "scene_open",
        json!({"path": "scenes/main.scene.ron"}),
    )
    .await;
    client
}

async fn ok(client: &McpClient, tool: &str, args: Value) -> Value {
    client
        .call(tool, args.clone())
        .await
        .unwrap_or_else(|e| panic!("{tool} {args}: {}", e.text))
}

/// Records holding W for 600 ticks.
async fn record_walk(client: &McpClient) {
    ok(
        client,
        "replay_record",
        json!({"action": "start", "path": REPLAY}),
    )
    .await;
    let hold = json!([{"tick": 0, "event": {"Key": {"key": "W", "pressed": true}}}]);
    ok(client, "input_inject", json!({"events": hold})).await;
    ok(client, "sim_step", json!({"ticks": 600})).await;
    ok(client, "replay_record", json!({"action": "stop"})).await;
}

fn near(v: &Value, want: f64) -> bool {
    v.as_f64().is_some_and(|x| (x - want).abs() < 1e-3)
}

#[tokio::test]
async fn seeded_bug_fails_reach_door_assertion_with_witness() {
    let dir = tempfile::tempdir().unwrap();
    let root = game(dir.path(), false);
    let client = server(&root, 1).await;
    let out = ok(
        &client,
        "assert_eval",
        json!({"source": "scenario", "path": PLAYTEST}),
    )
    .await;
    let r = &out["results"][0];
    assert_eq!(
        (&r["status"], &r["decided_at"]),
        (&json!("failed"), &json!(600)),
        "{out}"
    );
    assert!(
        near(&r["witness"]["closest"], 10.0 - 600.0 * 5.5 / 3600.0),
        "{r}"
    );
    assert_eq!(r["witness"]["closest_at"], 600, "{r}");
    client.shutdown().await;
}

#[tokio::test]
async fn fixed_script_passes_reach_door_assertion() {
    let dir = tempfile::tempdir().unwrap();
    let root = game(dir.path(), true);
    let client = server(&root, 1).await;
    let out = ok(
        &client,
        "assert_eval",
        json!({"source": "scenario", "path": PLAYTEST}),
    )
    .await;
    let r = &out["results"][0];
    assert_eq!(
        (&r["status"], &r["decided_at"]),
        (&json!("passed"), &json!(88)),
        "{out}"
    );
    let key = r#"distance(tag("player"), tag("door"))"#;
    assert!(near(&r["witness"][key], 10.0 - 88.0 * 5.5 / 60.0), "{r}");
    client.shutdown().await;
}

/// QA's way: a replay of the walk recorded on the buggy build fails the assertion; after the fix
/// (rebuilt and reloaded) the same inputs, resimulated and re-blessed, pass it at tick 88.
#[tokio::test]
async fn qa_style_replay_resimulates_and_passes_on_fixed_build() {
    let dir = tempfile::tempdir().unwrap();
    let root = game(dir.path(), false);
    let client = server(&root, 1).await;
    record_walk(&client).await;
    let replay = json!({"assertions": [REACH], "source": "replay", "path": REPLAY});
    let before = ok(&client, "assert_eval", replay.clone()).await;
    assert_eq!(before["results"][0]["status"], "failed", "{before}");
    m3_fixture::apply_fix(&root);
    m3_fixture::build(&root);
    ok(&client, "script_reload", json!({})).await;
    // The recording names the buggy module, so it is re-blessed on the fixed one (same inputs).
    let resim = ok(
        &client,
        "replay_run",
        json!({"path": REPLAY, "mode": "resimulate", "update": true, "force": true}),
    )
    .await;
    assert_eq!(resim["ticks"], 600, "{resim}");
    let after = client.call("assert_eval", replay).await;
    let after = match after {
        Ok(v) => v,
        Err(e) => panic!("assert_eval over the replay on the fixed build: {}", e.text),
    };
    assert_eq!(after["results"][0]["status"], "passed", "{after}");
    assert_eq!(after["results"][0]["decided_at"], 88, "{after}");
    client.shutdown().await;
}

/// A recorded walk verifies tick for tick on fresh servers at 1 and 8 threads.
#[tokio::test]
async fn replays_verify_with_zero_divergence_at_1_and_8_threads() {
    let dir = tempfile::tempdir().unwrap();
    let root = game(dir.path(), true);
    let recorder = server(&root, 1).await;
    record_walk(&recorder).await;
    recorder.shutdown().await;
    let mut hashes = Vec::new();
    for threads in [1, 8] {
        let client = server(&root, threads).await;
        let run = ok(&client, "replay_run", json!({"path": REPLAY})).await;
        assert_eq!(
            (&run["ok"], &run["divergence"]),
            (&json!(true), &Value::Null),
            "{run}"
        );
        hashes.push(run["world_hash"].clone());
        client.shutdown().await;
    }
    assert_eq!(hashes[0], hashes[1]);
}

/// `ab` against the fixed build names the first divergent tick and the player module's system.
#[tokio::test]
async fn ab_replay_bug_vs_fix_reports_first_divergent_tick_in_player_controller() {
    let dir = tempfile::tempdir().unwrap();
    let root = game(&dir.path().join("bug"), false);
    let fixed = game(&dir.path().join("fix"), true);
    let client = server(&root, 1).await;
    record_walk(&client).await;
    let run = ok(
        &client,
        "replay_run",
        json!({"path": REPLAY, "mode": "ab", "other": fixed.to_str().unwrap(), "force": true}),
    )
    .await;
    assert_eq!(run["ok"], false, "{run}");
    let d = &run["divergence"];
    assert_eq!(
        (&d["tick"], &d["last_matching_tick"]),
        (&json!(1), &json!(0)),
        "{run}"
    );
    assert_eq!(d["parts"], json!(["Transform"]), "{run}");
    assert_eq!(d["suspect_system"], "player.walk", "{run}");
    client.shutdown().await;
}

/// The fixture's hand-written scene is canonical: saved again by the engine (with the player
/// module loaded) it is byte for byte the same.
#[tokio::test]
async fn fixture_scene_is_canonical() {
    let dir = tempfile::tempdir().unwrap();
    let root = game(dir.path(), false);
    let client = server(&root, 1).await;
    ok(
        &client,
        "scene_save",
        json!({"path": "scenes/again.scene.ron"}),
    )
    .await;
    let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap();
    assert_eq!(
        read("scenes/again.scene.ron"),
        read("scenes/main.scene.ron")
    );
    client.shutdown().await;
}
