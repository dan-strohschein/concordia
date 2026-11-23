//! The `audio.*` methods and the `audio` debug layer. Expected values, by hand:
//! - `audio.render` of `loop` (DC 0.5, both channels) for 2 s: 96,000 frames; peak and RMS are
//!   0.5, 20·log₁₀(0.5) = −6.0206 dBFS; the hash is BLAKE3 of 192,000 little-endian 0.5s; the
//!   WAV holds 16384 (0.5 × 32768) in each of its 192,000 16-bit samples.
//! - `once` (100 frames of 0.5) for 1 s: RMS √(100 · 2 · 0.25 / 96,000) = 0.022822 → −32.8330.
//! - The play world's render after three ticks of a looping emitter: 2,400 frames, −6.0206.
//! - SFX at −6.0206 dB halves 0.5 to 0.25 (−12.0412 dBFS on SFX and Master); a second call with
//!   nothing played since reports −80 (silence).
//! - The debug layer: a positioned emitter draws 2 circles × 24 segments; the listener 3 axes;
//!   each playing positioned voice 1 line from the listener: 48 + 3 + 1 = 52 lines. Idle, it
//!   draws 48 + 3 and the circles are grey.

#[path = "rig/mod.rs"]
mod rig;

use concordia_api::{ApiErrorKind, EngineModule, WorldSel};
use concordia_audio::{AUDIO_LAYER, Backend, CIRCLE_SEGMENTS};
use rig::{Rig, at, close};
use serde_json::json;

#[test]
fn audio_methods_are_registered_by_name() {
    let names: Vec<_> = concordia_audio::AudioModule::new(Default::default())
        .unwrap()
        .methods()
        .iter()
        .map(|m| (m.name, m.mutates))
        .collect();
    assert_eq!(
        names,
        [
            ("audio.play", false),
            ("audio.stop", false),
            ("audio.buses", false),
            ("audio.render", false),
            ("audio.inspect", false)
        ]
    );
}

#[test]
fn audio_render_measures_a_cue_and_writes_a_wav() {
    let mut rig = Rig::new(1, Backend::Offline);
    let looped = rig.cues.looped.clone();
    let r = rig.ok(
        "audio.render",
        json!({"cue": looped, "seconds": 2.0, "path": "out/loop.wav"}),
    );
    assert_eq!(r["frames"], 96_000);
    assert!(
        (r["peak_dbfs"].as_f64().unwrap() + 6.0206).abs() < 1e-3,
        "{r}"
    );
    assert!(
        (r["rms_dbfs"].as_f64().unwrap() + 6.0206).abs() < 1e-3,
        "{r}"
    );
    let bytes: Vec<u8> = (0..192_000).flat_map(|_| 0.5f32.to_le_bytes()).collect();
    assert_eq!(r["hash"], blake3::hash(&bytes).to_hex().to_string());
    assert_eq!(r["path"], "out/loop.wav");
    let wav = hound::WavReader::open(rig.dir.path().join("out/loop.wav")).unwrap();
    assert_eq!(
        (wav.spec().channels, wav.spec().sample_rate, wav.len()),
        (2, 48_000, 192_000)
    );
    assert!(wav.into_samples::<i16>().all(|s| s.unwrap() == 16384));

    let once = rig.cues.once.clone();
    let r = rig.ok("audio.render", json!({"cue": once, "seconds": 1.0}));
    assert!(
        (r["rms_dbfs"].as_f64().unwrap() + 32.833).abs() < 1e-3,
        "{r}"
    );
    assert_eq!(r["path"], serde_json::Value::Null);
}

#[test]
fn audio_render_reports_the_play_world_and_refuses_bad_params() {
    let mut rig = Rig::new(1, Backend::Offline);
    let looped = rig.cues.looped.clone();
    rig.spawn(json!({"AudioEmitter": {"cue": looped}}));
    rig.steps(3);
    let r = rig.ok("audio.render", json!({"path": "out/play.wav"}));
    assert_eq!(r["frames"], 2400);
    assert!(
        (r["peak_dbfs"].as_f64().unwrap() + 6.0206).abs() < 1e-3,
        "{r}"
    );
    let wav = hound::WavReader::open(rig.dir.path().join("out/play.wav")).unwrap();
    assert_eq!(wav.len(), 4800);
    for (params, pointer) in [
        (
            json!({"cue": "asset:0190b5c0-0000-7000-8000-00000000dead"}),
            "/cue",
        ),
        (json!({"cue": looped, "seconds": 0.0}), "/seconds"),
        (json!({"cue": looped, "seconds": 601.0}), "/seconds"),
        (json!({"cue": looped, "path": "../out.wav"}), "/path"),
        (json!({"cue": looped, "path": "out.mp3"}), "/path"),
    ] {
        let e = rig.call("audio.render", params.clone()).unwrap_err();
        assert_eq!(e.kind, ApiErrorKind::InvalidParams, "{params}: {e:?}");
        assert_eq!(e.data.pointer.as_deref(), Some(pointer), "{params}: {e:?}");
    }
}

#[test]
fn audio_play_previews_outside_the_world_and_audio_stop_ends_it() {
    let mut rig = Rig::new(1, Backend::Offline);
    rig.spawn(json!({"AudioListener": {}, "Transform": at(0.0, 0.0, 0.0)}));
    let first = rig.step();
    let looped = rig.cues.looped.clone();
    let near = rig.cues.near.clone();
    let voice = rig.ok("audio.play", json!({"cue": looped}))["voice"]
        .as_u64()
        .unwrap();
    let positioned = rig.ok("audio.play", json!({"cue": near, "at": [2.0, 0.0, 0.0]}))["voice"]
        .as_u64()
        .unwrap();
    assert_ne!(voice, positioned);
    let inspect = rig.ok("audio.inspect", json!({}));
    let voices = inspect["voices"].as_array().unwrap();
    assert_eq!(voices.len(), 2, "{inspect}");
    assert_eq!(voices[0]["entity"], serde_json::Value::Null);
    assert_eq!(voices[0]["bus"], "SFX");
    rig.steps(1);
    // Tick 1: the unpositioned preview (0.5 both) plus the positioned one (right 0.25).
    close(&rig.left()[800..1600], &[0.5; 800]);
    close(&rig.right()[800..1600], &[0.75; 800]);
    assert_eq!(rig.ok("audio.stop", json!({"voice": voice}))["stopped"], 1);
    assert_eq!(rig.ok("audio.stop", json!({}))["stopped"], 1);
    rig.steps(1);
    assert_eq!(
        rig.left()[1600 + 240..]
            .iter()
            .filter(|s| **s != 0.0)
            .count(),
        0
    );
    let e = rig
        .call(
            "audio.play",
            json!({"cue": "asset:0190b5c0-0000-7000-8000-00000000dead"}),
        )
        .unwrap_err();
    assert_eq!(e.kind, ApiErrorKind::InvalidParams);
    // Previews never touch the world: the next hash only differs by the clock's tick.
    let mut quiet = Rig::new(1, Backend::Offline);
    quiet.spawn(json!({"AudioListener": {}, "Transform": at(0.0, 0.0, 0.0)}));
    assert_eq!(quiet.step(), first);
    quiet.steps(2);
    assert_eq!(quiet.step(), rig.step());
}

#[test]
fn audio_buses_report_and_change_gains_and_peaks() {
    let mut rig = Rig::new(1, Backend::Offline);
    let listed = rig.ok("audio.buses", json!({}));
    let names: Vec<_> = listed["buses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["name"].clone(), b["send"].clone()))
        .collect();
    assert_eq!(
        names,
        [
            (json!("Master"), json!(null)),
            (json!("SFX"), json!("Master")),
            (json!("Music"), json!("Master"))
        ]
    );
    let looped = rig.cues.looped.clone();
    rig.spawn(json!({"AudioEmitter": {"cue": looped}}));
    let set = rig.ok(
        "audio.buses",
        json!({"set": [{"name": "SFX", "gain_db": -6.0206}]}),
    );
    assert!((set["buses"][1]["gain_db"].as_f64().unwrap() + 6.0206).abs() < 1e-4);
    rig.steps(1);
    close(&rig.left()[..800], &[0.25; 800]);
    let peaks = rig.ok("audio.buses", json!({}));
    for bus in [0, 1] {
        assert!(
            (peaks["buses"][bus]["peak_dbfs"].as_f64().unwrap() + 12.0412).abs() < 1e-3,
            "{peaks}"
        );
    }
    assert_eq!(peaks["buses"][2]["peak_dbfs"], -80.0);
    let again = rig.ok("audio.buses", json!({}));
    assert_eq!(again["buses"][1]["peak_dbfs"], -80.0);
    rig.ok(
        "audio.buses",
        json!({"set": [{"name": "SFX", "mute": true}]}),
    );
    rig.steps(1);
    close(&rig.left()[800..1600], &[0.0; 800]);
    for bad in [
        json!({"set": [{"name": "Nope", "mute": true}]}),
        json!({"set": [{"name": "SFX", "gain_db": 30.0}]}),
    ] {
        let e = rig.call("audio.buses", bad.clone()).unwrap_err();
        assert_eq!(e.kind, ApiErrorKind::InvalidParams, "{bad}");
    }
}

#[test]
fn audio_inspect_reports_the_output_listener_and_counts() {
    let mut rig = Rig::new(1, Backend::Offline);
    let near = rig.cues.near.clone();
    let listener = rig.spawn(json!({"AudioListener": {}, "Transform": at(1.0, 0.0, 0.0)}));
    let e = rig.spawn(json!({"AudioEmitter": {"cue": near}, "Transform": at(3.0, 0.0, 0.0)}));
    rig.steps(1);
    let r = rig.ok("audio.inspect", json!({}));
    assert_eq!(r["backend"], "offline");
    assert_eq!(r["cues"], 4, "loop, once, near and vary");
    assert_eq!(r["listener"]["entity"], listener);
    assert_eq!(r["listener"]["position"], json!([1.0, 0.0, 0.0]));
    assert_eq!(r["listener"]["forward"], json!([0.0, 0.0, -1.0]));
    let voice = &r["voices"][0];
    assert_eq!(voice["entity"], e);
    assert_eq!(voice["position"], json!([3.0, 0.0, 0.0]));
    assert_eq!(voice["gains"], json!([0.0, 0.5]));
    assert_eq!(
        (
            r["steals"].clone(),
            r["drops"].clone(),
            r["underruns"].clone()
        ),
        (json!(0), json!(0), json!(0))
    );
    let none = Rig::new(1, Backend::None).ok("audio.inspect", json!({}));
    assert_eq!(none["backend"], "none");
}

#[test]
fn audio_debug_layer_draws_emitters_and_listener() {
    let mut rig = Rig::new(1, Backend::Offline);
    let near = rig.cues.near.clone();
    rig.spawn(json!({"AudioListener": {}, "Transform": at(0.0, 0.0, 0.0)}));
    let e = rig.spawn(json!({"AudioEmitter": {"cue": near}, "Transform": at(2.0, 0.0, 0.0)}));
    rig.steps(1);
    let module = concordia_audio::AudioModule::new(Default::default()).unwrap();
    assert_eq!(module.debug_layers(), [AUDIO_LAYER]);
    let draw = |rig: &Rig| {
        rig.engine
            .debug_geometry(AUDIO_LAYER, WorldSel::play)
            .unwrap()
    };
    let g = draw(&rig);
    assert_eq!(g.lines.len(), 2 * CIRCLE_SEGMENTS + 3 + 1);
    let lifted = g.lines[..2 * CIRCLE_SEGMENTS]
        .iter()
        .all(|l| (l.from.y - 0.02).abs() < 1e-6);
    assert!(lifted);
    // The outer circle's radius is max_distance (50 m), the inner min_distance (1 m).
    let radius = |l: &concordia_api::DebugLine| {
        (l.from - concordia_core::math::Vec3::new(2.0, 0.02, 0.0)).length()
    };
    assert!((radius(&g.lines[0]) - 1.0).abs() < 1e-4);
    assert!((radius(&g.lines[CIRCLE_SEGMENTS]) - 50.0).abs() < 1e-3);
    let green = g.lines[0].color;
    rig.patch_play(&e, "AudioEmitter", json!({"playing": false}));
    rig.steps(1);
    let idle = draw(&rig);
    assert_eq!(idle.lines.len(), 2 * CIRCLE_SEGMENTS + 3);
    assert_ne!(idle.lines[0].color, green, "idle circles are grey");
}
