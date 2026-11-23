//! The mixer, sample by sample, with DC and ramp sources so every value is hand-computable.
//!
//! - Unpositioned voices aren't panned: mono goes to both channels unchanged, stereo plays as is.
//!   A positioned (spatial) stereo source is averaged to mono first, then takes its gains.
//! - Bus gain: −6.0206 dB is ×0.5, −12.0412 dB ×0.25, +6.0206 dB ×2.
//!   DC 0.25 + DC 0.125 on SFX at −6.0206 dB: (0.375)(0.5) = 0.1875.
//!   SFX −6.0206, Music −12.0412, Master +6.0206, DC 0.5 on each: 0.5·0.5·2 + 0.5·0.25·2 = 0.75.
//!   Muting Music leaves 0.5.
//! - Linear interpolation: a ramp of k/100 at 24 kHz plays at step 24000/48000 = 0.5, so the
//!   output is 0, 0.005, 0.01, 0.015, …; +12 semitones (×2) at 48 kHz skips every other frame:
//!   0, 0.02, 0.04, ….
//! - A 3-frame one-shot [0.1, 0.2, 0.3] plays those then silence. Looped between frames 1 and 3,
//!   it plays 0.1, 0.2, 0.3, 0.2, 0.3, ….
//! - Stopping DC 1 at frame 0: frame k is (240 − k)/240 for k < 240, then 0.
//! - Stealing with 2 voices: A = 1 (priority 1), B = 10 (1) → 11; C = 100 (2) steals the oldest
//!   lowest, A → 110; D = 1000 (0) is outranked by all and dropped → 110.
//! - A group of at most 2: X₁ = 1, X₂ = 10, X₃ = 100 → X₁ ramps out over 240 frames → 110.
//! - Spatial stereo [1, 0] centred (gains 0.707107 each): mono 0.5 → 0.353553 on both.

mod common;

use std::num::NonZeroU16;

use common::*;
use concordia_audio::{
    AudioError, BusGraph, BusIndex, BusLayout, LoadedSound, MASTER, PlayRequest, STOP_RAMP_FRAMES,
};

fn on(graph: &BusGraph, name: &str, sound: LoadedSound) -> PlayRequest {
    PlayRequest::new(sound, graph.index(name).unwrap())
}

#[test]
fn mixer_sums_voices_and_applies_bus_gain_db() {
    let g = layout(0.0, -6.0206, 0.0);
    let (mut m, mut h) = mixer(g.clone(), 8);
    h.play(on(&g, "SFX", dc(0.25, 64))).unwrap();
    h.play(on(&g, "SFX", dc(0.125, 64))).unwrap();
    let out = render(&mut m, 16);
    close(&out, &[0.1875; 32]);
}

#[test]
fn bus_routing_sends_sfx_and_music_to_master() {
    let g = layout(6.0206, -6.0206, -12.0412);
    let (mut m, mut h) = mixer(g.clone(), 8);
    h.play(on(&g, "SFX", dc(0.5, 4800))).unwrap();
    h.play(on(&g, "Music", dc(0.5, 4800))).unwrap();
    close(&left(&mut m, 8), &[0.75; 8]);
    let music = g.index("Music").unwrap();
    h.set_bus(music, -12.0412, true).unwrap();
    close(&left(&mut m, 8), &[0.5; 8]);
    assert_eq!(g.parent(music), Some(BusIndex::MASTER));
    assert_eq!(
        g.names(),
        &[MASTER.to_owned(), "SFX".into(), "Music".into()]
    );
}

#[test]
fn bus_layouts_are_validated_by_name() {
    assert_eq!(
        BusGraph::new(&BusLayout::default()).unwrap().names(),
        &[MASTER, "SFX", "Music"]
    );
    let cycle = BusGraph::new(&BusLayout {
        buses: vec![bus("A", 0.0, Some("B")), bus("B", 0.0, Some("A"))],
    })
    .unwrap_err();
    assert!(
        matches!(&cycle, AudioError::BusCycle { path } if path.contains(&"A".into()) && path.contains(&"B".into())),
        "{cycle}"
    );
    let unknown = BusGraph::new(&BusLayout {
        buses: vec![bus("A", 0.0, Some("Nope"))],
    })
    .unwrap_err();
    assert_eq!(
        unknown,
        AudioError::UnknownSend {
            bus: "A".into(),
            send: "Nope".into()
        }
    );
    let twice = BusGraph::new(&BusLayout {
        buses: vec![bus("A", 0.0, None), bus("A", 0.0, None)],
    })
    .unwrap_err();
    assert_eq!(twice, AudioError::DuplicateBus("A".into()));
    let master = BusGraph::new(&BusLayout {
        buses: vec![bus(MASTER, 0.0, Some("SFX")), bus("SFX", 0.0, None)],
    })
    .unwrap_err();
    assert!(
        matches!(master, AudioError::Bus { ref bus, .. } if bus == MASTER),
        "{master}"
    );
    let loud = BusGraph::new(&BusLayout {
        buses: vec![bus("A", 40.0, None)],
    })
    .unwrap_err();
    assert!(loud.to_string().contains("gain_db"), "{loud}");
    // A bus with no send goes to Master; a chain sums children first.
    let chain = BusGraph::new(&BusLayout {
        buses: vec![
            bus("Steps", -6.0206, Some("SFX")),
            bus("SFX", -6.0206, None),
        ],
    })
    .unwrap();
    let (mut m, mut h) = mixer(chain.clone(), 4);
    h.play(on(&chain, "Steps", dc(1.0, 64))).unwrap();
    close(&left(&mut m, 4), &[0.25; 4]);
}

#[test]
fn pitch_resamples_by_linear_interpolation() {
    let g = layout(0.0, 0.0, 0.0);
    let ramp: Vec<f32> = (0..100).map(|k| k as f32 / 100.0).collect();
    let (mut m, mut h) = mixer(g.clone(), 4);
    h.play(on(&g, MASTER, mono(24_000, ramp.clone()))).unwrap();
    close(&left(&mut m, 6), &[0.0, 0.005, 0.01, 0.015, 0.02, 0.025]);
    let (mut m, mut h) = mixer(g.clone(), 4);
    let mut octave = on(&g, MASTER, mono(48_000, ramp));
    octave.pitch = concordia_audio::semitones_to_ratio(12.0);
    h.play(octave).unwrap();
    close(&left(&mut m, 4), &[0.0, 0.02, 0.04, 0.06]);
}

#[test]
fn one_shots_end_and_loops_wrap() {
    let g = layout(0.0, 0.0, 0.0);
    let (mut m, mut h) = mixer(g.clone(), 4);
    h.play(on(&g, MASTER, mono(48_000, vec![0.1, 0.2, 0.3])))
        .unwrap();
    close(&left(&mut m, 6), &[0.1, 0.2, 0.3, 0.0, 0.0, 0.0]);
    assert_eq!(h.stats().active(), 0);
    let (mut m, mut h) = mixer(g.clone(), 4);
    let mut sound = mono(48_000, vec![0.1, 0.2, 0.3]);
    sound.loop_start = 1;
    sound.loop_end = Some(3);
    let mut looped = on(&g, MASTER, sound);
    looped.looping = true;
    h.play(looped).unwrap();
    close(&left(&mut m, 7), &[0.1, 0.2, 0.3, 0.2, 0.3, 0.2, 0.3]);
    assert_eq!(h.stats().active(), 1);
}

#[test]
fn stop_ramps_to_silence_in_5_ms() {
    assert_eq!(STOP_RAMP_FRAMES, 240);
    let g = layout(0.0, 0.0, 0.0);
    let (mut m, mut h) = mixer(g.clone(), 4);
    let v = h.play(on(&g, MASTER, dc(1.0, 4800))).unwrap();
    h.stop(v).unwrap();
    let mut expected: Vec<f32> = (0..240).map(|k| (240 - k) as f32 / 240.0).collect();
    expected.extend([0.0; 10]);
    close(&left(&mut m, 250), &expected);
    h.collect();
    assert_eq!(h.stats().active(), 0);
    // Stopping a voice that has ended is harmless.
    h.stop(v).unwrap();
}

#[test]
fn voice_limit_steals_lowest_priority_oldest() {
    let g = layout(0.0, 0.0, 0.0);
    let (mut m, mut h) = mixer(g.clone(), 2);
    let with = |v: f32, priority: u8| PlayRequest {
        priority,
        ..on(&g, MASTER, dc(v, 48_000))
    };
    h.play(with(1.0, 1)).unwrap();
    render(&mut m, 1);
    h.play(with(10.0, 1)).unwrap();
    close(&left(&mut m, 2), &[11.0, 11.0]);
    h.play(with(100.0, 2)).unwrap();
    close(&left(&mut m, 2), &[110.0, 110.0]);
    assert_eq!(h.stats().steals(), 1);
    h.play(with(1000.0, 0)).unwrap();
    close(&left(&mut m, 2), &[110.0, 110.0]);
    assert_eq!((h.stats().steals(), h.stats().drops()), (1, 1));
}

#[test]
fn max_instances_stops_the_oldest() {
    let g = layout(0.0, 0.0, 0.0);
    let (mut m, mut h) = mixer(g.clone(), 8);
    let group = Some((7, NonZeroU16::new(2).unwrap()));
    for v in [1.0, 10.0, 100.0] {
        h.play(PlayRequest {
            group,
            ..on(&g, MASTER, dc(v, 48_000))
        })
        .unwrap();
        render(&mut m, 1);
    }
    render(&mut m, 240);
    close(&left(&mut m, 2), &[110.0, 110.0]);
    assert_eq!(h.stats().steals(), 0);
}

#[test]
fn spatial_voice_attenuates_with_distance_and_pans() {
    use concordia_audio::{Listener, Spatial, spatial_gains};
    use concordia_core::math::{Quat, Vec3};
    let g = layout(0.0, 0.0, 0.0);
    let listener = Listener::new(Vec3::ZERO, Quat::IDENTITY);
    let s = Spatial {
        min_distance: 1.0,
        max_distance: 50.0,
    };
    let (mut m, mut h) = mixer(g.clone(), 4);
    let gains = spatial_gains(Some(&listener), Vec3::new(2.0, 0.0, 0.0), &s);
    let v = h
        .play(PlayRequest {
            spatial: true,
            gains,
            ..on(&g, MASTER, dc(1.0, 4800))
        })
        .unwrap();
    close(&render(&mut m, 2), &[0.0, 0.5, 0.0, 0.5]);
    h.set_gains(
        v,
        spatial_gains(Some(&listener), Vec3::new(0.0, 0.0, -4.0), &s),
    )
    .unwrap();
    close(&render(&mut m, 1), &[0.176_777, 0.176_777]);
    h.set_gains(
        v,
        spatial_gains(Some(&listener), Vec3::new(-3.0, 0.0, 0.0), &s),
    )
    .unwrap();
    close(&render(&mut m, 1), &[1.0 / 3.0, 0.0]);

    let (mut m, mut h) = mixer(g.clone(), 4);
    let centred = spatial_gains(Some(&listener), Vec3::ZERO, &s);
    h.play(PlayRequest {
        spatial: true,
        gains: centred,
        ..on(&g, MASTER, stereo(48_000, vec![1.0, 0.0, 1.0, 0.0]))
    })
    .unwrap();
    close(&render(&mut m, 1), &[0.353_553, 0.353_553]);
    let (mut m, mut h) = mixer(g.clone(), 4);
    h.play(on(&g, MASTER, stereo(48_000, vec![1.0, 0.0, 1.0, 0.0])))
        .unwrap();
    close(&render(&mut m, 1), &[1.0, 0.0]);
}

#[test]
fn bad_requests_are_refused_before_the_queue() {
    let g = layout(0.0, 0.0, 0.0);
    let (_m, mut h) = mixer(g.clone(), 4);
    let ok = || on(&g, MASTER, dc(1.0, 10));
    let refused = |r: Result<_, AudioError>| matches!(r, Err(AudioError::Request(_)));
    assert!(refused(h.play(PlayRequest { pitch: 0.0, ..ok() })));
    assert!(refused(h.play(PlayRequest {
        pitch: 17.0,
        ..ok()
    })));
    assert!(refused(h.play(PlayRequest {
        volume: f32::NAN,
        ..ok()
    })));
    assert!(refused(h.play(PlayRequest {
        volume: -1.0,
        ..ok()
    })));
    assert!(refused(h.play(PlayRequest {
        gains: [f32::INFINITY, 0.0],
        ..ok()
    })));
    let master_only = BusGraph::new(&BusLayout { buses: vec![] }).unwrap();
    let (_m1, mut h1) = mixer(master_only, 4);
    assert!(
        refused(h1.play(on(&g, "SFX", dc(1.0, 1)))),
        "a bus from another layout"
    );
    let mut sound = dc(1.0, 10);
    sound.loop_end = Some(11);
    assert!(refused(h.play(PlayRequest {
        looping: true,
        ..on(&g, MASTER, sound)
    })));
    let mut sound = dc(1.0, 10);
    sound.loop_start = 5;
    sound.loop_end = Some(5);
    assert!(refused(h.play(PlayRequest {
        looping: true,
        ..on(&g, MASTER, sound)
    })));
    assert!(h.set_bus(BusIndex::MASTER, f32::NAN, false).is_err());
    assert!(
        h.set_gains(concordia_audio::VoiceId(1), [f32::NAN, 0.0])
            .is_err()
    );
    assert!(matches!(
        concordia_audio::Mixer::new({
            let mut c = concordia_audio::MixerConfig::new(g.clone());
            c.voices = std::num::NonZeroUsize::new(257).unwrap();
            c
        }),
        Err(AudioError::Config(_))
    ));
}

#[test]
fn a_full_queue_is_an_error_not_a_block() {
    let g = layout(0.0, 0.0, 0.0);
    let (_m, mut h) = mixer(g.clone(), 4);
    let mut result = Ok(concordia_audio::VoiceId(0));
    for _ in 0..=concordia_audio::COMMAND_CAPACITY {
        result = h.play(on(&g, MASTER, dc(1.0, 1)));
    }
    assert_eq!(result.unwrap_err(), AudioError::QueueFull);
}

/// Written after `read_buses` / `write_buses` (they reuse `concordia_scene`'s canonical
/// documents): the default layout's text, and a bad file's error naming the bus.
#[test]
fn bus_layout_files_roundtrip_canonical() {
    use concordia_audio::{read_buses, write_buses};
    let text = write_buses(&BusLayout::default()).unwrap();
    insta::assert_snapshot!(text);
    assert_eq!(read_buses(&text).unwrap(), BusLayout::default());
    let e = read_buses(r#"(buses: [(name: "SFX", send: Some("Nope"))])"#).unwrap_err();
    assert_eq!(
        e,
        AudioError::UnknownSend {
            bus: "SFX".into(),
            send: "Nope".into()
        }
    );
    assert!(matches!(
        read_buses("(buses: [(name: 3)])"),
        Err(AudioError::Parse(_))
    ));
}
