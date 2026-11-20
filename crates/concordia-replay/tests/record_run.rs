//! Recording and replaying over the Engine API (M2 phase 2.8).

mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use common::{Opts, Rig, steering};
use concordia_api::{ApiErrorKind, WorldSel};
use concordia_core::log::LogRing;
use concordia_replay::read_replay;
use serde_json::{Value, json};
use tracing_subscriber::layer::SubscriberExt;

const PATH: &str = "tests/replays/steer.replay.ron";

fn translations(rig: &Rig) -> Vec<[f32; 3]> {
    let world = rig.engine.world(WorldSel::play).unwrap();
    world
        .entity_ids_sorted()
        .into_iter()
        .filter_map(|id| world.get::<concordia_scene::builtin::Transform>(id))
        .map(|t| t.translation.to_array())
        .collect()
}

#[test]
fn recording_stores_only_ticks_where_input_changed() {
    let mut rig = Rig::new();
    let (stopped, _) = rig.record(PATH, 60, steering(), 300);
    assert_eq!(stopped["state"], "stopped");
    assert_eq!(stopped["tick"], 300);
    let replay = read_replay(&rig.file(PATH)).unwrap();
    let ticks: Vec<u64> = replay.inputs.iter().map(|i| i.tick).collect();
    // Each change; a press or release also changes the next tick, when its just_* flag clears.
    // At 30 and 50 only camera_pan's value changes (it stays pressed), so 31 and 51 don't.
    assert_eq!(ticks, [10, 11, 30, 50, 90, 91, 100, 101]);
    assert!(
        !replay.inputs[0].actions.contains_key("select"),
        "only changed actions"
    );
    let checkpoints: Vec<u64> = replay.checkpoints.iter().map(|c| c.tick).collect();
    assert_eq!(checkpoints, [0, 60, 120, 180, 240, 300]);
    assert_eq!(stopped["inputs"], 8);
    assert_eq!(stopped["checkpoints"], 6);
    let header = &replay.header;
    assert_eq!(
        (
            header.scene.as_str(),
            header.seed,
            header.rate_hz,
            header.threads
        ),
        ("scenes/test.scene.ron", 5, 60, 1)
    );
    assert_eq!(header.recorded_by, "agent:qa");
    assert_eq!(header.modules.len(), 1, "the steer module's fingerprint");
    let edit = rig
        .engine
        .world(WorldSel::edit)
        .unwrap()
        .state_hash()
        .unwrap();
    assert_eq!(header.scene_hash, edit.to_string());
}

#[test]
fn replayed_input_moves_entities_exactly_as_recorded() {
    let mut rig = Rig::new();
    let (_, recorded_hash) = rig.record(PATH, 60, steering(), 240);
    let recorded = translations(&rig);
    assert_ne!(recorded[0], [0.0, 0.0, 0.0], "the input really moved them");

    rig.ok("sim.stop", json!({}));
    let run = rig.ok("replay.run", json!({"path": PATH}));
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["matched"], 5, "checkpoints 0, 60, 120, 180, 240: {run}");
    assert_eq!(run["divergence"], Value::Null);
    assert_eq!(run["world_hash"], recorded_hash.as_str());
    assert_eq!(run["ticks"], 240);
    assert_eq!(
        translations(&rig),
        recorded,
        "the replayed play world stays for queries"
    );
}

#[test]
fn replay_detects_divergence_and_reports_first_tick_and_component() {
    let mut rig = Rig::new();
    rig.record(PATH, 10, steering(), 200);
    // Nondeterminism on replay only: the tick with SystemCtx::tick 130 (clock 130 → 131).
    rig.steer.perturb_at.store(130, Ordering::SeqCst);
    let run = rig.ok("replay.run", json!({"path": PATH}));
    assert_eq!(run["ok"], false, "{run}");
    let divergence = &run["divergence"];
    assert_eq!(divergence["tick"], 140, "{run}");
    assert_eq!(divergence["last_matching_tick"], 130);
    assert_eq!(divergence["parts"], json!(["Transform"]));
    assert_ne!(divergence["expected"], divergence["actual"]);
    assert_eq!(run["matched"], 14, "ticks 0..=130 matched");
    assert_eq!(run["ticks"], 140, "verify stops at the divergence");
}

#[test]
fn replay_header_mismatch_is_refused_unless_overridden() {
    let mut rig = Rig::new();
    rig.record(PATH, 60, steering(), 120);

    *rig.steer.code.lock().unwrap() = "steer-v2".into();
    let err = rig.err("replay.run", json!({"path": PATH}));
    assert_eq!(err.kind, ApiErrorKind::ReplayMismatch, "{err:?}");
    assert_eq!(
        err.data.extra["mismatches"][0]["field"], "modules",
        "{err:?}"
    );
    *rig.steer.code.lock().unwrap() = "steer-v1".into();

    let mover = rig.ok("query.run", json!({"with": ["Name"]}))["rows"][0]["id"].clone();
    rig.ok(
        "component.set",
        json!({"entity": mover, "component": "Name", "value": "Renamed"}),
    );
    let err = rig.err("replay.run", json!({"path": PATH}));
    assert_eq!(err.kind, ApiErrorKind::ReplayMismatch);
    assert_eq!(
        err.data.extra["mismatches"][0]["field"], "scene_hash",
        "{err:?}"
    );
    assert!(
        err.data.suggestions.iter().any(|s| s.contains("force")),
        "{err:?}"
    );

    let forced = rig.ok("replay.run", json!({"path": PATH, "force": true}));
    assert_eq!(forced["mismatches"][0]["field"], "scene_hash", "{forced}");
    assert_eq!(
        forced["divergence"]["tick"], 0,
        "the scene differs from the start"
    );
    assert_eq!(forced["divergence"]["parts"], json!(["Name"]));

    let mut reseeded = Rig::in_dir(
        rig.dir,
        Opts {
            seed: 6,
            ..Opts::default()
        },
    );
    let err = reseeded.err("replay.run", json!({"path": PATH}));
    let fields: Vec<&str> = err.data.extra["mismatches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["field"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["seed"], "{err:?}");
}

#[test]
fn replay_with_unknown_actions_is_refused_naming_them() {
    let mut rig = Rig::new();
    rig.record(PATH, 60, steering(), 60);
    let file = rig.dir.path().join(PATH);
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text.replace("\"camera_pan\"", "\"fly\"")).unwrap();
    let err = rig.err("replay.run", json!({"path": PATH}));
    assert_eq!(err.kind, ApiErrorKind::ReplayMismatch, "{err:?}");
    let mismatch = &err.data.extra["mismatches"][0];
    assert_eq!(mismatch["field"], "inputs", "{err:?}");
    assert!(
        mismatch["recorded"].as_str().unwrap().contains("fly"),
        "{err:?}"
    );
}

#[test]
fn resimulate_ignores_recorded_checkpoints_and_reports_new_ones() {
    let mut rig = Rig::new();
    rig.record(PATH, 30, steering(), 120);
    let recorded = read_replay(&rig.file(PATH)).unwrap();
    rig.steer.perturb_at.store(45, Ordering::SeqCst);
    let run = rig.ok("replay.run", json!({"path": PATH, "mode": "resimulate"}));
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["divergence"], Value::Null);
    assert_eq!(run["ticks"], 120);
    let hashes: Vec<(u64, String)> = run["checkpoints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["tick"].as_u64().unwrap(),
                c["world_hash"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let ticks: Vec<u64> = hashes.iter().map(|(t, _)| *t).collect();
    assert_eq!(ticks, [0, 30, 60, 90, 120]);
    assert_eq!(
        hashes[1].1, recorded.checkpoints[1].world,
        "before the change"
    );
    assert_ne!(hashes[2].1, recorded.checkpoints[2].world, "after it");
}

#[test]
fn replay_record_and_run_are_logged_with_their_actor() {
    let ring = Arc::new(LogRing::new(NonZeroUsize::new(512).unwrap()));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let mut rig = Rig::with(Opts {
            ring: Some(Arc::clone(&ring)),
            ..Opts::default()
        });
        let history = rig.ok("txn.history", json!({}))["entries"].clone();
        rig.record(PATH, 60, steering(), 60);
        rig.ok(
            "replay.run",
            json!({"path": PATH, "actor": "agent:reviewer"}),
        );
        let records = rig.ok("log.tail", json!({"level": "info"}))["records"].clone();
        let find = |message: &str| {
            records
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["message"] == message)
                .unwrap_or_else(|| panic!("no {message:?} record: {records}"))
                .clone()
        };
        let recorded = find("replay recorded");
        assert_eq!(recorded["fields"]["actor"], "agent:qa");
        assert_eq!(recorded["fields"]["path"], PATH);
        let run = find("replay run");
        assert_eq!(run["fields"]["actor"], "agent:reviewer");
        assert_eq!(run["fields"]["ok"], "true", "log fields are text");
        assert_eq!(
            rig.ok("txn.history", json!({}))["entries"],
            history,
            "recording and replaying journal nothing"
        );
    });
}

#[test]
fn stopping_without_a_recording_is_an_error_naming_start() {
    let mut rig = Rig::new();
    let err = rig.err("replay.record", json!({"action": "stop"}));
    assert!(
        err.data.suggestions.iter().any(|s| s.contains("start")),
        "{err:?}"
    );

    rig.ok("replay.record", json!({"path": PATH}));
    rig.ok("sim.step", json!({"ticks": 5}));
    rig.ok("sim.stop", json!({}));
    let err = rig.err("replay.record", json!({"action": "stop"}));
    assert!(err.message.contains("discarded"), "{err:?}");
    assert!(!rig.dir.path().join(PATH).exists());
}

#[test]
fn replay_paths_must_be_project_relative_replay_files() {
    let mut rig = Rig::new();
    for bad in ["../x.replay.ron", "tests/x.ron"] {
        let err = rig.err("replay.record", json!({"path": bad}));
        assert_eq!(err.kind, ApiErrorKind::InvalidParams, "{bad}: {err:?}");
        let err = rig.err("replay.run", json!({"path": bad}));
        assert_eq!(err.kind, ApiErrorKind::InvalidParams, "{bad}: {err:?}");
    }
    let err = rig.err("replay.record", json!({"action": "start"}));
    assert_eq!(
        err.kind,
        ApiErrorKind::InvalidParams,
        "start needs a path: {err:?}"
    );
}

#[test]
fn a_recording_whose_play_world_failed_to_start_does_not_join_a_later_one() {
    let mut rig = Rig::new();
    rig.steer.fail_start.store(true, Ordering::SeqCst);
    let err = rig.err("replay.record", json!({"path": PATH}));
    assert_eq!(err.message, "steer failed", "{err:?}");
    rig.steer.fail_start.store(false, Ordering::SeqCst);
    rig.ok("sim.step", json!({"ticks": 3}));
    let err = rig.err("replay.record", json!({"action": "stop"}));
    assert!(err.message.contains("nothing is being recorded"), "{err:?}");
}
