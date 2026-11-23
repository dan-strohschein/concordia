//! The offline backend: sample-exact ticks, statistics, and bit-identical renders.
//!
//! - At 60 Hz a tick is 800 frames, so a voice queued before tick 3 renders from frame 2,400:
//!   the capture's first non-zero sample is at index 2 × 2,400 = 4,800.
//! - DC 0.5 on both channels for 1,000 frames: peak 0.5, RMS 0.5.
//! - A 10 s script (a looping cue, one-shots with seeded variation, a voice moving across the
//!   listener, a stream, and one-shots every 2 ticks overlapping enough to steal in a pool of 4)
//!   rendered twice gives the same samples and
//!   the same BLAKE3 hash.

mod common;

use common::*;
use concordia_assets::AssetRef;
use concordia_audio::{
    Cue, CueSound, DrawKey, Listener, LoadedBody, LoadedSound, MASTER, MixerConfig, OfflineBackend,
    PlayRequest, Span, Spatial, db_to_gain, decode_ogg, spatial_gains,
};
use concordia_core::math::{Quat, Vec3};

fn backend(voices: usize) -> (OfflineBackend, concordia_audio::BusGraph) {
    let g = layout(0.0, 0.0, 0.0);
    let mut config = MixerConfig::new(g.clone());
    config.voices = std::num::NonZeroUsize::new(voices).unwrap();
    (OfflineBackend::new(config).unwrap(), g)
}

#[test]
fn ticks_render_their_frames_and_commands_land_on_the_tick() {
    let (mut b, g) = backend(4);
    b.start_capture();
    for tick in 0..3 {
        b.render_tick(tick, hz(60));
    }
    assert_eq!(b.frames(), 2400);
    b.handle()
        .play(PlayRequest::new(dc(0.5, 1000), g.index(MASTER).unwrap()))
        .unwrap();
    b.render_tick(3, hz(60));
    assert_eq!(b.frames(), 3200);
    let capture = b.take_capture();
    assert_eq!(capture.iter().position(|x| *x != 0.0), Some(4800));
    // Rendering a tick already rendered adds nothing.
    b.render_tick(3, hz(60));
    assert_eq!(b.frames(), 3200);
}

#[test]
fn stats_report_peak_rms_and_hash() {
    let (mut b, g) = backend(4);
    b.handle()
        .play(PlayRequest::new(dc(0.5, 1000), g.index(MASTER).unwrap()))
        .unwrap();
    b.render_frames(1000);
    let s = b.stats();
    assert_eq!(s.frames, 1000);
    assert_eq!(s.peak, 0.5);
    assert!((s.rms - 0.5).abs() < 1e-6, "{}", s.rms);
    let mut bytes = Vec::new();
    for _ in 0..2000 {
        bytes.extend(0.5f32.to_le_bytes());
    }
    assert_eq!(s.hash, blake3::hash(&bytes).to_hex().to_string());
}

fn script() -> (String, Vec<f32>) {
    let (mut b, g) = backend(4);
    let master = g.index(MASTER).unwrap();
    b.start_capture();
    let blip = mono(
        48_000,
        (0..2400).map(|k| ((k % 48) as f32 / 48.0) - 0.5).collect(),
    );
    let hum = dc(0.05, 48_000);
    let cue = Cue {
        sounds: vec![
            CueSound {
                sound: "builtin:sound/a".parse::<AssetRef>().unwrap(),
                weight: 2,
            },
            CueSound {
                sound: "builtin:sound/b".parse::<AssetRef>().unwrap(),
                weight: 1,
            },
        ],
        volume_db: Span {
            min: -12.0,
            max: 0.0,
        },
        pitch_semitones: Span {
            min: -3.0,
            max: 3.0,
        },
        seed: 42,
        ..Cue::default()
    };
    let listener = Listener::new(Vec3::ZERO, Quat::IDENTITY);
    let spatial = Spatial::default();
    let stream = LoadedSound {
        body: LoadedBody::Stream(tone_bytes().into()),
        gain: 0.5,
        loop_start: 0,
        loop_end: None,
    };
    let h = b.handle();
    h.play(PlayRequest {
        looping: true,
        ..PlayRequest::new(hum, master)
    })
    .unwrap();
    h.play(PlayRequest {
        looping: true,
        ..PlayRequest::new(stream, master)
    })
    .unwrap();
    let mover = h
        .play(PlayRequest {
            looping: true,
            spatial: true,
            ..PlayRequest::new(
                LoadedSound::memory(decode_ogg(&tone_bytes()).unwrap()),
                master,
            )
        })
        .unwrap();
    for tick in 0..600u64 {
        let x = -10.0 + tick as f32 / 30.0;
        let h = b.handle();
        h.set_gains(
            mover,
            spatial_gains(Some(&listener), Vec3::new(x, 0.0, -2.0), &spatial),
        )
        .unwrap();
        if tick % 2 == 0 {
            let d = cue.draw(DrawKey {
                entity: 5,
                tick,
                n: 0,
            });
            let sound = if d.sound == 0 {
                blip.clone()
            } else {
                dc(0.2, 1200)
            };
            h.play(PlayRequest {
                volume: db_to_gain(d.volume_db),
                pitch: d.pitch,
                priority: (tick % 3) as u8,
                ..PlayRequest::new(sound, master)
            })
            .unwrap();
        }
        b.render_tick(tick, hz(60));
    }
    assert!(b.handle().stats().steals() > 0, "the script must steal");
    (b.stats().hash, b.take_capture())
}

#[test]
fn offline_backend_renders_bit_identical_buffers() {
    let (hash, samples) = script();
    let (again, more) = script();
    assert_eq!(samples.len(), 600 * 800 * 2);
    assert!(samples.iter().any(|x| *x != 0.0));
    assert_eq!(hash, again);
    assert!(
        samples
            .iter()
            .zip(&more)
            .all(|(a, b)| a.to_bits() == b.to_bits())
    );
}
