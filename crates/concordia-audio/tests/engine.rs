//! The `audio` module in an engine: emitters, `PlayCue`, the listener, the world-hash boundary,
//! replays, and a missing device. Expected values, by hand (48 kHz, 60 Hz, so 800 frames a
//! tick; the play world's first tick is 0; sounds are DC 0.5, so a held sample is 0.5):
//! - An unpositioned looping emitter playing from the start sounds 0.5 on both channels from
//!   frame 0. Turned off before tick 3, tick 3's first frame starts the ramp: frame 2400 + k is
//!   0.5 (240 − k)/240 for k < 240, then silence. On again before tick 4: 0.5 from frame 3200.
//!   Despawned by gameplay in tick 5: the same ramp from frame 4000.
//! - A `PlayCue` raised in tick 2 is delivered in tick 3, so `once` (100 frames) sounds on
//!   frames 2400..2500 and nowhere else.
//! - Listener at the origin, unturned; `near` at (2, 0, 0): distance 2 → 0.5, fully right →
//!   right 0.5 × 0.5 = 0.25, left 0. A second listener with a higher id at (4, 0, 0) would put
//!   it on the left, so the right channel shows which one is heard. Listener gain 0.5 halves it
//!   (0.125). No listener: as if at the listener, centred, 0.5 × 0.707107 = 0.353553 each.

#[path = "rig/mod.rs"]
mod rig;

use std::sync::Arc;

use concordia_audio::{AudioDevice, AudioError, Backend, OpenDevice};
use rig::{Rig, at, close};
use serde_json::json;

fn ramp_from(start: usize) -> impl Fn(usize) -> f32 {
    move |frame| {
        let k = frame - start;
        if k < 240 {
            0.5 * (240 - k) as f32 / 240.0
        } else {
            0.0
        }
    }
}

#[test]
fn emitter_plays_while_playing_and_stops_with_a_ramp() {
    let mut rig = Rig::new(1, Backend::Offline);
    let cue = rig.cues.looped.clone();
    let e = rig.spawn(json!({"AudioEmitter": {"cue": cue}, "test.Doom": {"at": 5}}));
    rig.steps(3);
    close(&rig.left(), &[0.5; 2400]);
    rig.patch_play(&e, "AudioEmitter", json!({"playing": false}));
    rig.steps(1);
    let off: Vec<f32> = (2400..3200).map(ramp_from(2400)).collect();
    close(&rig.left()[2400..3200], &off);
    rig.patch_play(&e, "AudioEmitter", json!({"playing": true}));
    rig.steps(1);
    close(&rig.left()[3200..4000], &[0.5; 800]);
    // `test.Doom` despawns it in tick 5's PostPhysics.
    rig.steps(1);
    let gone: Vec<f32> = (4000..4800).map(ramp_from(4000)).collect();
    close(&rig.left()[4000..4800], &gone);
    assert_eq!(rig.right(), rig.left(), "unpositioned: both channels");
    assert!(rig.audio().lock().playing().is_empty());
}

#[test]
fn play_cue_signal_plays_once_at_the_next_tick() {
    let mut rig = Rig::new(1, Backend::Offline);
    let cue = rig.cues.once.clone();
    rig.spawn(json!({"test.Bell": {"cue": cue, "at": [2]}}));
    rig.steps(6);
    let left = rig.left();
    assert_eq!(left.len(), 4800);
    close(&left[2400..2500], &[0.5; 100]);
    assert!(left[..2400].iter().chain(&left[2500..]).all(|s| *s == 0.0));
    assert!(
        rig.audio().lock().playing().is_empty(),
        "the one-shot ended"
    );
}

#[test]
fn listener_is_the_lowest_id_and_missing_listener_centres_sound() {
    let mut rig = Rig::new(1, Backend::Offline);
    let near = rig.cues.near.clone();
    rig.spawn(json!({"AudioEmitter": {"cue": near}, "Transform": at(2.0, 0.0, 0.0)}));
    let first = rig.spawn(json!({"Transform": at(0.0, 0.0, 0.0)}));
    let second = rig.spawn(json!({"Transform": at(4.0, 0.0, 0.0)}));
    rig.steps(1);
    close(&rig.left()[..800], &[0.353_553; 800]);
    close(&rig.right()[..800], &[0.353_553; 800]);
    rig.set_play(&second, "AudioListener", json!({}));
    rig.set_play(&first, "AudioListener", json!({}));
    rig.steps(1);
    close(&rig.left()[800..1600], &[0.0; 800]);
    close(&rig.right()[800..1600], &[0.25; 800]);
    rig.patch_play(&first, "AudioListener", json!({"gain": 0.5}));
    rig.steps(1);
    close(&rig.right()[1600..2400], &[0.125; 800]);
    let (entity, _) = rig.audio().lock().listener().unwrap();
    assert_eq!(entity.to_string(), first);
}

/// Thirty ticks with a looping emitter, a moving positioned emitter and bells on ticks 2, 9
/// and 20, with `audio.collect` scheduled (offline) and without it (`Backend::None`): every
/// tick's world hash is the same, at 1 and 8 threads and run to run.
#[test]
fn audio_is_presentation_only_and_never_changes_the_world_hash() {
    let run = |threads: usize, backend: Backend| {
        let mut rig = Rig::new(threads, backend);
        let (looped, once, near) = (
            rig.cues.looped.clone(),
            rig.cues.once.clone(),
            rig.cues.near.clone(),
        );
        rig.spawn(json!({"AudioEmitter": {"cue": looped, "volume_db": -6.0}}));
        let mover =
            rig.spawn(json!({"AudioEmitter": {"cue": near}, "Transform": at(-5.0, 0.0, 0.0)}));
        rig.spawn(json!({"AudioListener": {}, "Transform": at(0.0, 0.0, 0.0)}));
        rig.spawn(
            json!({"test.Bell": {"cue": once, "at": [2, 9, 20]}, "Transform": at(1.0, 0.0, 1.0)}),
        );
        let mut hashes = Vec::new();
        for tick in 0..30 {
            if tick == 10 {
                rig.patch_play(&mover, "Transform", json!({"translation": [5.0, 0.0, 0.0]}));
            }
            hashes.push(rig.step());
        }
        let heard = rig.audio().lock().render_stats().map(|s| s.peak > 0.0);
        (hashes, heard)
    };
    let (expected, heard) = run(1, Backend::Offline);
    assert_eq!(heard, Some(true), "sound played");
    for threads in [1, 8] {
        assert_eq!(
            run(threads, Backend::None).0,
            expected,
            "{threads} threads, no audio"
        );
        assert_eq!(
            run(threads, Backend::Offline).0,
            expected,
            "{threads} threads, audio"
        );
    }
}

#[test]
fn engine_audio_renders_identically_across_runs_threads_and_replay() {
    let render = |threads: usize| {
        let mut rig = Rig::with(
            threads,
            Backend::Offline,
            vec![Arc::new(concordia_replay::ReplayModule::new())],
        );
        let (once, near) = (rig.cues.once.clone(), rig.cues.near.clone());
        rig.spawn(json!({"AudioEmitter": {"cue": near}, "Transform": at(3.0, 0.0, -1.0)}));
        rig.spawn(json!({"AudioListener": {}, "Transform": at(0.0, 0.0, 0.0)}));
        rig.spawn(json!({"test.Bell": {"cue": once, "at": [1, 4, 7, 30]}}));
        rig.ok(
            "replay.record",
            json!({"path": "tests/replays/a.replay.ron", "actor": rig::ACTOR}),
        );
        rig.steps(60);
        rig.ok(
            "replay.record",
            json!({"action": "stop", "actor": rig::ACTOR}),
        );
        let recorded = rig.audio_hash();
        let run = rig.ok("replay.run", json!({"path": "tests/replays/a.replay.ron"}));
        assert_eq!(run["ok"], true, "{run}");
        (
            recorded,
            rig.audio_hash(),
            rig.audio().lock().render_stats().unwrap().frames,
        )
    };
    let (recorded, replayed, frames) = render(1);
    assert_eq!(frames, 60 * 800);
    assert_eq!(replayed, recorded, "the replay renders the same audio");
    assert_eq!(render(8).0, recorded, "8 threads");
    assert_eq!(render(1).0, recorded, "run to run");
}

#[derive(Debug)]
struct NoDevice;

impl AudioDevice for NoDevice {
    fn open(
        &self,
        _: &concordia_audio::BusGraph,
        _: std::num::NonZeroUsize,
    ) -> Result<OpenDevice, AudioError> {
        Err(AudioError::Config(
            "audio device: no default output device".into(),
        ))
    }
}

#[test]
fn missing_device_warns_and_falls_back_to_offline() {
    use tracing_subscriber::layer::SubscriberExt;
    let ring = Arc::new(concordia_core::log::LogRing::new(
        std::num::NonZeroUsize::new(64).unwrap(),
    ));
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::with_default(subscriber, || {
        let mut rig = Rig::new(1, Backend::Device(Arc::new(NoDevice)));
        assert_eq!(rig.audio().lock().backend_name(), "offline");
        let cue = rig.cues.looped.clone();
        rig.spawn(json!({"AudioEmitter": {"cue": cue}}));
        rig.steps(2);
        close(&rig.left(), &[0.5; 1600]);
    });
    let warnings: Vec<_> = ring
        .tail(None)
        .into_iter()
        .filter(|r| {
            r.level == concordia_core::log::LogLevel::Warn && r.message.contains("no audio device")
        })
        .collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

/// Two plays from one entity on one tick draw with n = 0 and n = 1, so a varied cue gets two
/// different volumes (and previews count their own ticks, so two previews differ too).
#[test]
fn two_plays_on_one_tick_draw_differently() {
    let mut rig = Rig::new(1, Backend::Offline);
    let vary = rig.cues.vary.clone();
    rig.spawn(json!({"test.Bell": {"cue": vary, "at": [2, 2]}}));
    rig.steps(4);
    let inspect = rig.ok("audio.inspect", json!({}));
    let volumes: Vec<f64> = inspect["voices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["volume_db"].as_f64().unwrap())
        .collect();
    assert_eq!(volumes.len(), 2, "{inspect}");
    assert_ne!(volumes[0], volumes[1], "{volumes:?}");
    assert!(
        volumes.iter().all(|v| (-12.0..=0.0).contains(v)),
        "{volumes:?}"
    );
    let a = rig.ok("audio.play", json!({"cue": rig.cues.vary}))["voice"]
        .as_u64()
        .unwrap();
    let b = rig.ok("audio.play", json!({"cue": rig.cues.vary}))["voice"]
        .as_u64()
        .unwrap();
    let inspect = rig.ok("audio.inspect", json!({}));
    let volume = |voice: u64| {
        inspect["voices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["voice"] == voice)
            .unwrap()["volume_db"]
            .as_f64()
            .unwrap()
    };
    assert_ne!(volume(a), volume(b), "previews draw differently");
}

/// Written after the code: a bell rings `loop` in tick 1 (sounding from tick 2, frame 1600) and
/// a hush raises `StopCue` in tick 4, delivered in tick 5: the ramp runs from frame 4000.
#[test]
fn stop_cue_stops_what_the_entity_started() {
    let mut rig = Rig::new(1, Backend::Offline);
    let cue = rig.cues.looped.clone();
    rig.spawn(json!({"test.Bell": {"cue": cue, "at": [1]}, "test.Hush": {"at": 4}}));
    rig.steps(6);
    let left = rig.left();
    assert!(left[..1600].iter().all(|s| *s == 0.0));
    close(&left[1600..4000], &[0.5; 2400]);
    let ramp: Vec<f32> = (4000..4800).map(ramp_from(4000)).collect();
    close(&left[4000..4800], &ramp);
    assert!(rig.audio().lock().playing().is_empty());
}

fn bus_gain(rig: &mut Rig, bus: &str) -> f64 {
    let buses = rig.ok("audio.buses", json!({}));
    buses["buses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == bus)
        .unwrap_or_else(|| panic!("no bus {bus}: {buses}"))["gain_db"]
        .as_f64()
        .unwrap()
}

#[test]
fn set_bus_gain_signal_sets_a_bus_gain_from_gameplay() {
    // Gameplay can't call `audio.buses`, so a volume slider raises `SetBusGain` (M2 phase
    // 2.10c.3, gap G1). Raised on tick 2, delivered on tick 3: after ticks 0..=2 Music is still
    // 0 dB, after tick 3 it is −12 dB. An unknown bus changes nothing and warns once.
    use tracing_subscriber::layer::SubscriberExt;
    let ring = Arc::new(concordia_core::log::LogRing::new(
        std::num::NonZeroUsize::new(64).unwrap(),
    ));
    // Process-wide (nextest runs each test in its own process): the collector may run on a job
    // thread, which a thread-scoped subscriber wouldn't see.
    let subscriber = tracing_subscriber::registry().with(ring.layer());
    tracing::subscriber::set_global_default(subscriber).unwrap();
    {
        let mut rig = Rig::new(1, Backend::Offline);
        rig.spawn(json!({"test.Fader": {"bus": "Music", "gain_db": -12.0, "at": 2}}));
        rig.spawn(json!({"test.Fader": {"bus": "Nope", "gain_db": -6.0, "at": 2}}));
        rig.steps(3);
        assert_eq!(bus_gain(&mut rig, "Music"), 0.0);
        rig.steps(1);
        assert_eq!(bus_gain(&mut rig, "Music"), -12.0);
        assert_eq!(bus_gain(&mut rig, "SFX"), 0.0, "only Music changed");
    }
    let warnings: Vec<_> = ring
        .tail(None)
        .into_iter()
        .filter(|r| r.level == concordia_core::log::LogLevel::Warn && r.message.contains("Nope"))
        .collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}
