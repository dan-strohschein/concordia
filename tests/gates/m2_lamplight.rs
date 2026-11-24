//! M2 gate (deferred from M2 2.10d; M3 3.9a): Lamplight played over MCP stdio with only the
//! player's hands (`input_inject`, `sim_step`) and reads (`query_run`, `ui_inspect`).
//!
//! By hand (60 Hz; the lamplighter walks 3.5 m/s; a lamp in reach takes 20 of the can's 100 oil;
//! see `tests/integration/lamplight_night.rs` for the rule timings):
//! - **The won night:** type "Ada", four ↓ on the night length (180 → 60 s), Start. Walk to the
//!   two south ring lamps (posts at (±4.59, 11.09), r = 12 at 67.5° and 112.5°) and the two
//!   nearest harbour lamps ((±6.5, 19)), standing 1.2 m toward the street, and light each: the can
//!   holds 100 − 4 × 20 = 20. Stand on the ring road (z = 16) until a wagon stops beside the
//!   lamplighter and fills the can back to 100. The lit lamps cover the wisps' way in from the
//!   harbour, so wisps burn (burn numbers rise over them) and the square keeps some spirit; dawn
//!   comes at 60 s (tick ≈ 3,620 with the title-screen ticks), so the night is `won` with
//!   4 lamps lit, and the end screen's records list names Ada.
//! - **The idle night:** Start at the default 180 s and do nothing: three waves of six wisps by
//!   45 s (the first at 5 s, then every 20 s) reach the square, each costing 1 of 10 spirit, so
//!   the night is `lost` with the clock well before 180 s.
//! - **Replays:** the won night recorded from tick 0 verifies tick for tick on fresh servers at
//!   1 and 8 threads, and all three runs end on the same world hash.
//! - **Hot reload:** `script_reload` of the native fog module mid-night keeps the night playing:
//!   the clock keeps counting and the next wave still comes.
//! - **The mix:** a wave burning in one lamp plays up to six sizzles and six pops at once on the
//!   SFX bus (−3 dB), so the night's mix stays below 0 dBFS.
//! - Every test ends with no error logs.

#[path = "support/lamplight.rs"]
mod lamplight;

use base64::Engine as _;
use concordia_testkit::golden::{RgbaImage, Tolerance, assert_golden};
use concordia_testkit::gpu_required;
use lamplight::{Town, project};
use serde_json::{Value, json};

const REPLAY: &str = "tests/replays/won-night.replay.ron";

/// Starts a night from the title screen as Ada, `downs` ↓ on the night length (4: 60 s).
async fn start_as_ada(town: &mut Town, downs: u32) {
    town.step(2).await;
    town.click("Name box").await;
    town.type_text("Ada").await;
    town.click("Night length").await;
    for _ in 0..downs {
        town.tap("ArrowDown").await;
    }
    town.click("Start").await;
    town.step(3).await;
}

/// Walks to `stand` beside lamp `index`, presses E and waits for the `lit` event (70 ticks).
async fn light(town: &mut Town, stand: [f64; 2]) {
    town.walk_to(stand, 0.25).await;
    town.tap("E").await;
    town.step(70).await;
}

/// How many burn numbers float in the play world now.
async fn burn_numbers(town: &Town) -> usize {
    town.all("lamplight.Number", "play")
        .await
        .iter()
        .filter(|(_, c)| c["Name"] == "Burn Number")
        .count()
}

/// The chase camera, for captures.
async fn camera(town: &Town) -> String {
    town.one("lamplight.Chase", "play").await.0
}

/// Captures 1280 × 720 through the chase camera and compares it with `tests/golden/gates/<name>`
/// (FLIP); `false` when there is no GPU here (allowed unless `CONCORDIA_REQUIRE_GPU=1`).
async fn golden(town: &Town, name: &str) -> bool {
    let cam = camera(town).await;
    let args = json!({"camera": cam, "width": 1280, "height": 720, "world": "play"});
    let result =
        serde_json::to_value(town.client.call_raw("capture_screenshot", args).await).unwrap();
    if result["isError"] == true {
        assert!(
            result.to_string().contains("GpuUnavailable") && !gpu_required(),
            "capture failed: {result}"
        );
        return false;
    }
    let data = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "image")
        .expect("an image")["data"]
        .as_str()
        .unwrap()
        .to_owned();
    let png = base64::engine::general_purpose::STANDARD
        .decode(data)
        .unwrap();
    let image = RgbaImage::from_png(&png).expect("a PNG");
    let tolerance = Tolerance {
        mean: 0.02,
        p99: 0.3,
    };
    assert_golden("gates", name, &image, tolerance);
    true
}

/// The mix so far, rounded so it compares across runs and platforms: peak and RMS to 1 dB, and
/// each bus's peak since the last read.
async fn mix_digest(town: &Town) -> Value {
    let mix = town.ok("audio_render", json!({})).await;
    let buses = town.ok("audio_buses", json!({})).await;
    let db = |v: &Value| v.as_f64().map(|x| x.round());
    let buses: Vec<Value> = buses["buses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| json!({"name": b["name"], "peak_dbfs": db(&b["peak_dbfs"])}))
        .collect();
    json!({"peak_dbfs": db(&mix["peak_dbfs"]), "rms_dbfs": db(&mix["rms_dbfs"]), "buses": buses})
}

/// Plays the won night on `town`, recording it; returns the final world hash and the most burn
/// numbers seen at once.
async fn play_won_night(town: &mut Town, captures: &mut Vec<bool>) -> (String, usize) {
    town.ok("replay_record", json!({"action": "start", "path": REPLAY}))
        .await;
    start_as_ada(town, 4).await;
    assert_eq!(town.night().await["phase"], "playing");
    for stand in [[3.4, 11.5], [-3.4, 11.5], [-5.3, 19.0], [5.3, 19.0]] {
        light(town, stand).await;
    }
    assert_eq!(town.lamplighter().await.0["can"], 20.0, "four lamps lit");
    town.walk_to([0.0, 16.0], 0.3).await;
    let mut most = 0;
    while town.lamplighter().await.0["can"] == 20.0 && town.tick < 3000 {
        town.step(60).await;
        most = most.max(burn_numbers(town).await);
    }
    assert_eq!(
        town.lamplighter().await.0["can"],
        100.0,
        "a wagon refilled the can"
    );
    captures.push(golden(town, "m2_lamplight_mid_night").await);
    let mut hash = String::new();
    while town.night().await["phase"] == "playing" && town.tick < 4500 {
        hash = town.step(60).await;
        most = most.max(burn_numbers(town).await);
    }
    town.ok("replay_record", json!({"action": "stop"})).await;
    (hash, most)
}

#[tokio::test]
async fn m2_gate_scripted_night_is_won_and_replays_exactly_at_1_and_8_threads() {
    let dir = project();
    let mut town = Town::open(dir.path(), 1).await;
    let mut captures = Vec::new();
    let (hash, most) = play_won_night(&mut town, &mut captures).await;
    let night = town.night().await;
    assert_eq!(night["phase"], "won", "{night}");
    assert!(night["lamps_lit"].as_u64().unwrap() >= 4, "{night}");
    assert!(night["wisps_burned"].as_u64().unwrap() > 0, "{night}");
    assert!(night["spirit"].as_u64().unwrap() > 0, "{night}");
    assert!(most > 0, "burn numbers rose over burning wisps");
    town.step(3).await;
    let end = town.named("End screen", "play").await.0;
    let screen = town
        .ok("ui_inspect", json!({"world": "play", "entity": end}))
        .await;
    assert!(
        screen.to_string().contains("Ada"),
        "the records list names Ada: {screen}"
    );
    captures.push(golden(&town, "m2_lamplight_dawn").await);
    let mix = mix_digest(&town).await;
    // Six sizzles and six pops at once when a wave burns in one lamp: the mix must not clip.
    assert!(
        mix["peak_dbfs"].as_f64().unwrap() < 0.0,
        "the night's mix clips: {mix}"
    );
    insta::assert_json_snapshot!("m2_lamplight_won_night_mix", mix);
    assert_eq!(town.errors().await, json!([]));
    town.client.shutdown().await;
    assert!(
        captures.iter().all(|c| *c == captures[0]),
        "all captures or none"
    );

    // Fresh servers at 1 and 8 threads verify the recording tick for tick, side by side, each on
    // its own copy of the project (one project's servers would share one engine).
    let copies: Vec<tempfile::TempDir> = (0..2).map(|_| tempfile::tempdir().unwrap()).collect();
    for copy in &copies {
        concordia_tests::copy_project(dir.path(), copy.path()).unwrap();
        let ddc = dir.path().join(".concordia/ddc");
        if ddc.is_dir() {
            concordia_tests::copy_project(&ddc, &copy.path().join(".concordia/ddc")).unwrap();
        }
    }
    let verify = |threads: usize, root: &std::path::Path| {
        let (root, hash) = (root.to_path_buf(), hash.clone());
        async move {
            let replayer = Town::open(&root, threads).await;
            let run = replayer.ok("replay_run", json!({"path": REPLAY})).await;
            assert_eq!(run["ok"], true, "{threads} threads: {run}");
            assert_eq!(
                run["world_hash"],
                hash.as_str(),
                "{threads} threads end on the same hash"
            );
            assert_eq!(replayer.errors().await, json!([]));
            replayer.client.shutdown().await;
        }
    };
    tokio::join!(verify(1, copies[0].path()), verify(8, copies[1].path()));
}

#[tokio::test]
async fn m2_gate_title_screen_golden() {
    let dir = project();
    let mut town = Town::open(dir.path(), 1).await;
    town.step(2).await;
    town.click("Name box").await;
    town.type_text("Ada").await;
    golden(&town, "m2_lamplight_title").await;
    assert_eq!(town.errors().await, json!([]));
    town.client.shutdown().await;
}

#[tokio::test]
async fn m2_gate_idle_night_is_lost() {
    let dir = project();
    let mut town = Town::open(dir.path(), 1).await;
    start_as_ada(&mut town, 0).await;
    while town.night().await["phase"] == "playing" && town.tick < 12_000 {
        town.step(300).await;
    }
    let night = town.night().await;
    assert_eq!(night["phase"], "lost", "{night}");
    assert_eq!(night["spirit"], 0, "{night}");
    assert!(
        night["clock"].as_f64().unwrap() < 180.0,
        "before dawn: {night}"
    );
    assert_eq!(town.errors().await, json!([]));
    town.client.shutdown().await;
}

#[tokio::test]
async fn m2_gate_hot_reload_keeps_the_night_running() {
    let dir = project();
    let mut town = Town::open(dir.path(), 1).await;
    start_as_ada(&mut town, 0).await;
    town.step(600).await;
    let before = town.night().await;
    let reloaded = town.ok("script_reload", json!({"module": "fog"})).await;
    assert!(reloaded.to_string().contains("fog"), "{reloaded}");
    town.step(900).await;
    let after = town.night().await;
    assert_eq!(after["phase"], "playing", "{after}");
    let (c0, c1) = (
        before["clock"].as_f64().unwrap(),
        after["clock"].as_f64().unwrap(),
    );
    assert!(
        (c1 - c0 - 15.0).abs() < 0.1,
        "the clock kept counting: {c0} → {c1}"
    );
    let wisps = town.all("fog.Drifter", "play").await.len();
    let lost = 10 - after["spirit"].as_u64().unwrap() as usize;
    assert!(
        wisps + lost + after["wisps_burned"].as_u64().unwrap() as usize > 6,
        "the wave at 25 s still came"
    );
    assert_eq!(town.errors().await, json!([]));
    town.client.shutdown().await;
}

/// The tool each role M2 unlocked uses in the sample (M2 2.10d).
const ROLE_TOOLS: [(&str, &str); 9] = [
    ("gameplay-programmer", "script_reload"),
    ("animator", "anim_inspect"),
    ("sound-designer", "audio_inspect"),
    ("composer", "audio_buses"),
    ("ui-ux", "ui_inspect"),
    ("tech-artist", "material_get"),
    ("level-designer", "terra_stats"),
    ("level-designer", "nav_path"),
    ("qa-playtester", "replay_run"),
];

/// Each role's tool works on the town and is in that role's default scope (served by this
/// server); the gate's client recorded calling every one.
#[tokio::test]
async fn m2_gate_every_m2_role_uses_a_tool_in_the_sample() {
    let dir = project();
    let mut town = Town::open(dir.path(), 1).await;
    town.ok(
        "replay_record",
        json!({"action": "start", "path": "tests/replays/short.replay.ron"}),
    )
    .await;
    start_as_ada(&mut town, 0).await;
    town.hold("W", 30).await;
    town.ok("replay_record", json!({"action": "stop"})).await;
    let body = town.one("lamplight.Lamplighter", "play").await.1["lamplight.Lamplighter"]["body"]
        .as_str()
        .unwrap()
        .to_owned();
    let pose = town.ok("anim_inspect", json!({"entity": body})).await;
    assert!(!pose["bones"].as_array().unwrap().is_empty(), "{pose}");
    let voices = town.ok("audio_inspect", json!({})).await;
    assert!(
        voices["voices"].as_array().is_some_and(|v| !v.is_empty()),
        "the music plays: {voices}"
    );
    assert!(
        town.ok("audio_buses", json!({})).await["buses"]
            .as_array()
            .unwrap()
            .len()
            == 5
    );
    let hud = town.named("HUD", "play").await.0;
    assert!(
        town.ok("ui_inspect", json!({"world": "play", "entity": hud}))
            .await
            .to_string()
            .contains("Lamps lit")
    );
    let cobbles = town
        .ok(
            "material_get",
            json!({"path": "materials/wet_cobbles.material.ron"}),
        )
        .await;
    assert!(
        cobbles["material"]["nodes"]
            .as_object()
            .is_some_and(|n| n.len() > 1),
        "{cobbles}"
    );
    let stats = town.ok("terra_stats", json!({"world": "play"})).await;
    assert!(stats["entities"].as_u64().unwrap() > 40, "{stats}");
    let path = town
        .ok(
            "nav_path",
            json!({"from": [0.0, 0.0, 12.0], "to": [0.0, 0.0, 4.5], "world": "play"}),
        )
        .await;
    assert_eq!(path["partial"], false, "the square is reachable: {path}");
    let reloaded = town.ok("script_reload", json!({"module": "fog"})).await;
    assert!(
        reloaded["modules"]
            .as_array()
            .is_some_and(|m| !m.is_empty())
    );
    let run = town
        .ok(
            "replay_run",
            json!({"path": "tests/replays/short.replay.ron"}),
        )
        .await;
    assert_eq!(run["ok"], true, "{run}");

    // The sample as a Studio project (the team tools and the generators join its own), served
    // only to list the tools the default roles resolve against.
    let studio = project();
    let manifest = studio.path().join("concordia.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, format!("{text}\n[gen]\n\n[studio]\n")).unwrap();
    concordia_studio::init(studio.path(), std::path::Path::new("studio")).unwrap();
    let lister = Town::open(studio.path(), 1).await;
    let served: Vec<String> = lister
        .client
        .tools()
        .await
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    lister.client.shutdown().await;
    let catalog = concordia_studio::ToolCatalog::from_names(served);
    let roster = concordia_studio::Roster::load(&studio.path().join("studio"), &catalog).unwrap();
    let called = town.called();
    for (role, tool) in ROLE_TOOLS {
        let scope = &roster
            .role(role)
            .unwrap_or_else(|| panic!("{role} is enabled"))
            .mcp_tools;
        assert!(
            scope.iter().any(|t| t == tool),
            "{role} may call {tool}: {scope:?}"
        );
        assert!(called.contains(tool), "the gate called {tool}");
    }
    assert_eq!(town.errors().await, json!([]));
    town.client.shutdown().await;
}
