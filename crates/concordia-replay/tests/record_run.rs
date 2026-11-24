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
    // The input map, the steer module's code (module order), then the template catalog (3.1a).
    let names: Vec<String> = header
        .modules
        .iter()
        .map(|m| format!("{}/{}", m.module, m.name))
        .collect();
    assert_eq!(names, ["input/map", "steer/steer", "engine/templates"]);
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
    // Per-tick hashes (v1) name the exact tick, 131; parts are compared at the next checkpoint.
    let divergence = &run["divergence"];
    assert_eq!(divergence["tick"], 131, "{run}");
    assert_eq!(divergence["last_matching_tick"], 130);
    assert_eq!(divergence["parts_tick"], 140);
    assert_eq!(divergence["parts"], json!(["Transform"]));
    assert_ne!(divergence["expected"], divergence["actual"]);
    assert_eq!(run["matched"], 14, "ticks 0..=130 matched");
    assert_eq!(
        run["ticks"], 140,
        "verify stops at the checkpoint after the divergence"
    );
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

/// The m1 map plus a ray action, `aim`, which the steer module adds to every velocity.
const RAY_MAP: &str = r#"InputMap(
    actions: {
        "camera_pan": (kind: Axis2d, bindings: [KeyAxis2d(up: W, down: S, left: A, right: D)]),
        "aim": (kind: Ray, bindings: [PointerRay]),
    },
)"#;

#[test]
fn replays_record_and_play_back_pointer_rays() {
    let mut rig = Rig::with(Opts {
        map: RAY_MAP,
        ..Opts::default()
    });
    let ray = |tick: u64, d: [f32; 3]| json!({"tick": tick, "event": {"PointerRay": {"origin": [0.0, 10.0, 0.0], "direction": d}}});
    let events = json!([
        ray(5, [1.0, -1.0, 0.0]),
        ray(40, [0.0, -1.0, 2.0]),
        ray(80, [-3.0, 0.0, 0.0])
    ]);
    let (_, recorded_hash) = rig.record(PATH, 30, events, 120);
    let text = rig.file(PATH);
    // Ticks 5, 40 and 80 bring new rays; tick 6 is recorded too, as `just_pressed` falls.
    assert_eq!(text.matches("Ray(").count(), 4, "{text}");
    let recorded = translations(&rig);
    assert_ne!(recorded[0], [0.0, 0.0, 0.0], "the rays really moved them");

    rig.ok("sim.stop", json!({}));
    let run = rig.ok("replay.run", json!({"path": PATH}));
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["world_hash"], recorded_hash.as_str());
    assert_eq!(translations(&rig), recorded);
}

/// Typed text is recorded on the tick it was typed (2.7b.3) and played back: a replay of a
/// session that typed "ok" on tick 5 and a Backspace on tick 7 stores both and verifies.
#[test]
fn typed_text_is_recorded_and_replayed() {
    let mut rig = Rig::new();
    let typed = json!([
        {"tick": 5, "event": {"Text": {"Insert": "ok"}}},
        {"tick": 7, "event": {"Text": {"Edit": {"key": "Backspace", "select": false}}}},
    ]);
    rig.record(PATH, 10, typed, 20);
    let replay = read_replay(&rig.file(PATH)).unwrap();
    let text: Vec<(u64, usize)> = replay
        .inputs
        .iter()
        .filter(|i| !i.text.is_empty())
        .map(|i| (i.tick, i.text.len()))
        .collect();
    assert_eq!(text, [(5, 1), (7, 1)]);
    let run = rig.ok("replay.run", json!({"path": PATH}));
    assert_eq!(run["ok"], true, "{run}");
    let world = rig.engine.world(WorldSel::play).unwrap();
    assert!(
        world.resource::<concordia_input::TypedText>().is_some(),
        "the play world publishes typed text"
    );
}

/// `InputActions` (2.10a) is hashed play state filled from input, so a replay reproduces it:
/// over 45 ticks of `steering()` (W from 10, D from 30) a watcher's mirror shows `camera_pan`
/// pressed with x > 0 and y > 0 at the end, and the replayed world's mirror and hash are the
/// recording's. Written after `input.actions` (a guard for 2.10a's replay claim).
#[test]
fn input_actions_replay_exactly() {
    let mut rig = Rig::new();
    let watcher = rig.ok(
        "entity.spawn",
        json!({"components": {"InputActions": {"watch": ["camera_pan", "select"]}}}),
    )["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let mirror = |rig: &Rig| {
        rig.engine
            .world(WorldSel::play)
            .unwrap()
            .get::<concordia_input::InputActions>(watcher)
            .cloned()
            .unwrap()
    };
    let (_, recorded_hash) = rig.record(PATH, 15, steering(), 45);
    let recorded = mirror(&rig);
    let pan = recorded.states["camera_pan"];
    assert!(pan.pressed, "{recorded:?}");
    match pan.value {
        concordia_input::ActionValue::Axis2d(v) => assert!(v.x > 0.0 && v.y > 0.0, "{v}"),
        other => panic!("{other:?}"),
    }

    rig.ok("sim.stop", json!({}));
    let run = rig.ok("replay.run", json!({"path": PATH}));
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["world_hash"], recorded_hash.as_str());
    assert_eq!(mirror(&rig), recorded);
}

/// Per-tick hashes find the exact tick with sparse checkpoints (M3 3.1a): checkpoints every 64
/// ticks over 600, and a nudge on the tick whose `SystemCtx::tick` is 416 changes the hash after
/// tick 417. Ticks 1..=416 match, the divergence is at 417, and the parts are compared at the
/// next checkpoint, 448. Checkpoints 0, 64, …, 384 match: 7.
#[test]
fn per_tick_hashes_locate_the_first_divergent_tick_exactly() {
    let mut rig = Rig::new();
    rig.record(PATH, 64, steering(), 600);
    rig.steer.perturb_at.store(416, Ordering::SeqCst);
    let run = rig.ok("replay.run", json!({"path": PATH}));
    let d = &run["divergence"];
    assert_eq!(
        (&d["tick"], &d["last_matching_tick"], &d["parts_tick"]),
        (&json!(417), &json!(416), &json!(448)),
        "{run}"
    );
    assert_eq!(d["parts"], json!(["Transform"]));
    assert_eq!(run["matched"], 7);
}

/// A recording names its platform (`<OS>-<arch>`), the manifest's hash (BLAKE3 of the empty
/// input here, as the test project has no concordia.toml: af1349b9…3262), its task, and every
/// tick's hash: 300 ticks fill ⌈300 / 64⌉ = 5 lines, and at each checkpoint tick the per-tick
/// entry is the checkpoint's world hash cut to 16 digits.
#[test]
fn recordings_name_platform_manifest_task_and_every_tick() {
    let mut rig = Rig::new();
    rig.ok(
        "replay.record",
        json!({"action": "start", "path": PATH, "checkpoint_every": 60, "task": "T-0042"}),
    );
    rig.ok("input.inject", json!({ "events": steering() }));
    rig.ok("sim.step", json!({ "ticks": 300 }));
    rig.ok("replay.record", json!({"action": "stop"}));
    let replay = read_replay(&rig.file(PATH)).unwrap();
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    assert_eq!(replay.header.platform.as_deref(), Some(platform.as_str()));
    assert_eq!(
        replay.header.manifest_hash.as_deref(),
        Some("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262")
    );
    assert_eq!(replay.header.task.as_deref(), Some("T-0042"));
    assert_eq!(replay.hashes.len(), 5);
    for c in replay.checkpoints.iter().filter(|c| c.tick > 0) {
        assert_eq!(
            replay.tick_hash(c.tick),
            Some(&c.world[..16]),
            "tick {}",
            c.tick
        );
    }
}

/// A run refuses another platform, tick rate or manifest (M3 3.1a), naming the field.
#[test]
fn platform_rate_or_manifest_change_is_a_mismatch() {
    let mut rig = Rig::new();
    rig.record(PATH, 60, steering(), 120);
    let file = rig.dir.path().join(PATH);
    let text = std::fs::read_to_string(&file).unwrap();
    let field = |rig: &mut Rig| {
        let err = rig.err("replay.run", json!({"path": PATH}));
        assert_eq!(err.kind, ApiErrorKind::ReplayMismatch, "{err:?}");
        err.data.extra["mismatches"][0]["field"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    std::fs::write(&file, text.replace(&platform, "plan9-z80")).unwrap();
    assert_eq!(field(&mut rig), "platform");
    std::fs::write(&file, text.replace("rate_hz: 60", "rate_hz: 30")).unwrap();
    assert_eq!(field(&mut rig), "rate_hz");
    std::fs::write(&file, &text).unwrap();
    std::fs::write(rig.dir.path().join("concordia.toml"), "[replay]\n").unwrap();
    assert_eq!(field(&mut rig), "manifest_hash");
}

/// Content is part of what a replay ran (M3 3.1a): the input map and the template catalog are
/// fingerprinted beside the code, so a different map or a new template is a `modules` mismatch
/// naming `input/map` or `engine/templates`.
#[test]
fn input_map_and_template_changes_are_mismatches() {
    let mut rig = Rig::new();
    rig.record(PATH, 60, steering(), 60);
    let replay = read_replay(&rig.file(PATH)).unwrap();
    let names: Vec<String> = replay
        .header
        .modules
        .iter()
        .map(|m| format!("{}/{}", m.module, m.name))
        .collect();
    assert!(names.contains(&"input/map".to_owned()), "{names:?}");
    assert!(names.contains(&"engine/templates".to_owned()), "{names:?}");

    std::fs::create_dir_all(rig.dir.path().join("templates")).unwrap();
    std::fs::write(
        rig.dir.path().join("templates/rock.template.ron"),
        "Template(format: 1, description: \"A rock.\", traits: {}, components: {\"Name\": \"Rock\"})\n",
    )
    .unwrap();
    let err = rig.err("replay.run", json!({"path": PATH}));
    let m = &err.data.extra["mismatches"][0];
    assert_eq!(m["field"], "modules", "{err:?}");
    assert!(
        m["current"].as_str().unwrap().contains("engine/templates"),
        "{err:?}"
    );
    std::fs::remove_dir_all(rig.dir.path().join("templates")).unwrap();

    let mut remapped = Rig::in_dir(
        rig.dir,
        Opts {
            map: RAY_MAP,
            ..Opts::default()
        },
    );
    let err = remapped.err("replay.run", json!({"path": PATH}));
    let m = &err.data.extra["mismatches"][0];
    assert!(
        m["current"].as_str().unwrap().contains("input/map"),
        "{err:?}"
    );
}

/// A version 0 replay still verifies (M3 3.1a): the recording rewritten as version 0 (no
/// platform, manifest hash, task or per-tick hashes) runs to the same final hash, comparing
/// checkpoints only.
#[test]
fn a_version_0_replay_verifies_with_the_same_final_hash() {
    let mut rig = Rig::new();
    let (_, recorded_hash) = rig.record(PATH, 30, steering(), 120);
    let file = rig.dir.path().join(PATH);
    let v0 = concordia_tests::replays::as_version_0(&std::fs::read_to_string(&file).unwrap());
    std::fs::write(&file, &v0).unwrap();
    let run = rig.ok("replay.run", json!({"path": PATH}));
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["world_hash"], recorded_hash.as_str());
    assert_eq!(run["matched"], 5, "checkpoints 0, 30, 60, 90 and 120");
}
