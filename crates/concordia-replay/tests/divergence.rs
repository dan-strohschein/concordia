//! Field-level divergence reports from checkpoint stills, re-blessing with `update`, and
//! comparing two replay files (M3 phase 3.1b, decision R2: stills for diffs, never restored).
//!
//! The rig's three movers start at x = 0, 1, 2. The steering presses W on ticks 10..50 and D on
//! 30..90, so `camera_pan` drives every Velocity. A nudge on the tick whose `SystemCtx::tick` is
//! 130 adds 0.25 to the first entity's `Transform.translation.x` on replay only, so the first
//! divergent tick is 131 and, with checkpoints every 10, the parts are compared at 140: there the
//! first mover's x is 0.25 larger than recorded and nothing else differs.

mod common;

use std::sync::atomic::Ordering;

use common::{Rig, steering};
use concordia_replay::{ReplayDiff, compare_replays, read_replay};
use serde_json::{Value, json};

const PATH: &str = "tests/replays/steer.replay.ron";

fn first_mover(rig: &Rig) -> String {
    rig.engine
        .world(concordia_api::WorldSel::edit)
        .unwrap()
        .entity_ids_sorted()[0]
        .to_string()
}

#[test]
fn divergence_report_names_entity_component_and_field() {
    let mut rig = Rig::new();
    rig.record(PATH, 10, steering(), 200);
    rig.steer.perturb_at.store(130, Ordering::SeqCst);
    let run = rig.ok("replay.run", json!({"path": PATH}));
    let d = &run["divergence"];
    assert_eq!(
        (&d["tick"], &d["parts_tick"]),
        (&json!(131), &json!(140)),
        "{run}"
    );
    let fields = d["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 1, "{run}");
    let f = &fields[0];
    assert_eq!(f["entity"], first_mover(&rig).as_str());
    assert_eq!(
        (&f["component"], &f["path"]),
        (&json!("Transform"), &json!("translation/0"))
    );
    let (want, got) = (
        f["expected"].as_f64().unwrap(),
        f["actual"].as_f64().unwrap(),
    );
    assert!((got - want - 0.25).abs() < 1e-5, "{f}");
    assert_eq!(d["fields_note"], Value::Null);
}

/// Stills are a cache: without `.concordia/ddc` the divergence is still found, the fields are
/// empty and a note (and a warning) says why.
#[test]
fn missing_still_reports_without_values_and_says_why() {
    let mut rig = Rig::new();
    rig.record(PATH, 10, steering(), 200);
    std::fs::remove_dir_all(rig.dir.path().join(".concordia/ddc")).unwrap();
    rig.steer.perturb_at.store(130, Ordering::SeqCst);
    let run = rig.ok("replay.run", json!({"path": PATH}));
    let d = &run["divergence"];
    assert_eq!(d["tick"], 131, "{run}");
    assert_eq!(d["fields"], json!([]));
    let note = d["fields_note"].as_str().unwrap();
    assert!(
        note.contains(".concordia/ddc") && note.contains("140"),
        "{note}"
    );
}

/// `"stills": false` records no stills (checkpoints carry no `still` key), and a divergence then
/// notes that the recording has none.
#[test]
fn stills_can_be_left_out() {
    let mut rig = Rig::new();
    rig.ok(
        "replay.record",
        json!({"action": "start", "path": PATH, "checkpoint_every": 10, "stills": false}),
    );
    rig.ok("input.inject", json!({ "events": steering() }));
    rig.ok("sim.step", json!({ "ticks": 200 }));
    rig.ok("replay.record", json!({"action": "stop"}));
    let replay = read_replay(&rig.file(PATH)).unwrap();
    assert!(replay.checkpoints.iter().all(|c| c.still.is_none()));
    rig.steer.perturb_at.store(130, Ordering::SeqCst);
    let run = rig.ok("replay.run", json!({"path": PATH}));
    let note = run["divergence"]["fields_note"].as_str().unwrap();
    assert!(note.contains("without stills"), "{note}");
}

/// `resimulate` with `update: true` re-blesses a replay on the current code: after the steer
/// code changes (`steer-v2`, a different fingerprint, and a nudge at 45 that changes behaviour),
/// verify refuses the old file, `update` rewrites it with the same inputs and fresh hashes, and
/// verify then passes. `update` is refused in verify mode.
#[test]
fn resimulate_update_rewrites_the_replay_on_current_code() {
    let mut rig = Rig::new();
    rig.record(PATH, 30, steering(), 120);
    let before = read_replay(&rig.file(PATH)).unwrap();
    *rig.steer.code.lock().unwrap() = "steer-v2".into();
    rig.steer.perturb_at.store(45, Ordering::SeqCst);
    let err = rig.err("replay.run", json!({"path": PATH}));
    assert_eq!(err.kind, concordia_api::ApiErrorKind::ReplayMismatch);
    let err = rig.err("replay.run", json!({"path": PATH, "update": true}));
    assert_eq!(
        err.kind,
        concordia_api::ApiErrorKind::InvalidParams,
        "{err:?}"
    );
    let updated = rig.ok(
        "replay.run",
        json!({"path": PATH, "mode": "resimulate", "update": true, "force": true}),
    );
    assert_eq!(updated["ok"], true, "{updated}");
    let after = read_replay(&rig.file(PATH)).unwrap();
    assert_eq!(after.inputs, before.inputs, "the same inputs");
    assert_ne!(
        after.checkpoints[2].world, before.checkpoints[2].world,
        "new behaviour"
    );
    assert!(after.header.modules.iter().any(|m| m.hash == "steer-v2"));
    let run = rig.ok("replay.run", json!({"path": PATH}));
    assert_eq!(run["ok"], true, "{run}");
}

/// Two replays of the same scene compared offline: the nudged one (re-blessed from the first
/// with a nudge at 130) differs from tick 131, its parts at 140 name `Transform`, and the field
/// is the first mover's x. Identical files report no difference.
#[test]
fn compare_replays_names_the_first_differing_tick_and_fields() {
    const OTHER: &str = "tests/replays/nudged.replay.ron";
    let mut rig = Rig::new();
    rig.record(PATH, 10, steering(), 200);
    std::fs::copy(rig.dir.path().join(PATH), rig.dir.path().join(OTHER)).unwrap();
    rig.steer.perturb_at.store(130, Ordering::SeqCst);
    rig.ok(
        "replay.run",
        json!({"path": OTHER, "mode": "resimulate", "update": true}),
    );
    let a = read_replay(&rig.file(PATH)).unwrap();
    let b = read_replay(&rig.file(OTHER)).unwrap();
    let ddc = rig.dir.path().join(".concordia/ddc");
    assert_eq!(
        compare_replays(&a, &b, &ddc).first_input_tick,
        None,
        "the same inputs"
    );
    let same: ReplayDiff = compare_replays(&a, &a, &ddc);
    assert!(same.same(), "{same:?}");
    let diff = compare_replays(&a, &b, &ddc);
    assert!(!diff.same());
    assert_eq!(
        (diff.first_tick, diff.parts_tick),
        (Some(131), Some(140)),
        "{diff:?}"
    );
    assert_eq!(diff.parts, ["Transform"]);
    assert_eq!(diff.fields.len(), 1, "{diff:?}");
    assert_eq!(diff.fields[0].path, "translation/0");
}

/// `ab` runs the replay here and on `other` (a copy of this project built by the injected
/// factory, whose steer module nudges at 130) in lockstep: they first differ after tick 131,
/// exactly; the fields are diffed at that tick (here = expected, other = actual: x is 0.25
/// larger), and re-running tick 131 observed names the system whose wave first differs,
/// `steer.apply`.
#[test]
fn ab_mode_reports_the_first_divergence_and_the_suspect_system() {
    let mut rig = Rig::with(common::Opts {
        other_build_nudges_at: Some(130),
        ..common::Opts::default()
    });
    rig.record(PATH, 10, steering(), 200);
    let other = tempfile::tempdir().unwrap();
    concordia_tests::copy_project(rig.dir.path(), other.path()).unwrap();
    let run = rig.ok(
        "replay.run",
        json!({"path": PATH, "mode": "ab", "other": other.path().to_str().unwrap()}),
    );
    assert_eq!(run["ok"], false, "{run}");
    let d = &run["divergence"];
    assert_eq!(
        (&d["tick"], &d["last_matching_tick"], &d["parts_tick"]),
        (&json!(131), &json!(130), &json!(131)),
        "{run}"
    );
    assert_eq!(d["parts"], json!(["Transform"]));
    assert_eq!(d["suspect_system"], "steer.apply", "{run}");
    let f = &d["fields"][0];
    assert_eq!(
        (&f["component"], &f["path"]),
        (&json!("Transform"), &json!("translation/0"))
    );
    let (want, got) = (
        f["expected"].as_f64().unwrap(),
        f["actual"].as_f64().unwrap(),
    );
    assert!((got - want - 0.25).abs() < 1e-5, "{f}");
}

/// Two identical builds agree for the whole replay; `ab` needs `other`, and an engine factory
/// (the app injects one; a bare module has none).
#[test]
fn ab_mode_agrees_with_an_identical_build_and_needs_other_and_a_factory() {
    let mut rig = Rig::with(common::Opts {
        other_build_nudges_at: Some(u64::MAX),
        ..common::Opts::default()
    });
    rig.record(PATH, 10, steering(), 120);
    let other = tempfile::tempdir().unwrap();
    concordia_tests::copy_project(rig.dir.path(), other.path()).unwrap();
    let run = rig.ok(
        "replay.run",
        json!({"path": PATH, "mode": "ab", "other": other.path().to_str().unwrap()}),
    );
    assert_eq!(
        (&run["ok"], &run["ticks"]),
        (&json!(true), &json!(120)),
        "{run}"
    );
    let err = rig.err("replay.run", json!({"path": PATH, "mode": "ab"}));
    assert_eq!(
        err.kind,
        concordia_api::ApiErrorKind::InvalidParams,
        "{err:?}"
    );
    let mut bare = Rig::new();
    bare.record(PATH, 10, steering(), 20);
    let err = bare.err(
        "replay.run",
        json!({"path": PATH, "mode": "ab", "other": other.path().to_str().unwrap()}),
    );
    assert!(err.message.contains("factory"), "{err:?}");
}
