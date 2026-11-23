//! Shared sources and mixers for the mixer tests.
#![allow(dead_code, missing_docs)]

use std::num::{NonZeroU32, NonZeroUsize};
use std::path::Path;

use concordia_audio::{
    BusDef, BusGraph, BusLayout, Channels, LoadedSound, MASTER, Mixer, MixerConfig, MixerHandle,
    SoundData,
};

pub fn hz(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
}

/// `frames` of a constant at 48 kHz, mono.
pub fn dc(value: f32, frames: usize) -> LoadedSound {
    mono(48_000, vec![value; frames])
}

pub fn mono(rate: u32, samples: Vec<f32>) -> LoadedSound {
    LoadedSound::memory(SoundData::new(hz(rate), Channels::Mono, samples).unwrap())
}

pub fn stereo(rate: u32, samples: Vec<f32>) -> LoadedSound {
    LoadedSound::memory(SoundData::new(hz(rate), Channels::Stereo, samples).unwrap())
}

pub fn bus(name: &str, gain_db: f32, send: Option<&str>) -> BusDef {
    BusDef {
        name: name.into(),
        gain_db,
        mute: false,
        send: send.map(Into::into),
    }
}

/// Master, SFX and Music at the given gains.
pub fn layout(master: f32, sfx: f32, music: f32) -> BusGraph {
    BusGraph::new(&BusLayout {
        buses: vec![
            bus(MASTER, master, None),
            bus("SFX", sfx, Some(MASTER)),
            bus("Music", music, Some(MASTER)),
        ],
    })
    .unwrap()
}

pub fn mixer(buses: BusGraph, voices: usize) -> (Mixer, MixerHandle) {
    let mut config = MixerConfig::new(buses);
    config.voices = NonZeroUsize::new(voices).unwrap();
    Mixer::new(config).unwrap()
}

/// Renders `frames` and returns the left channel.
pub fn left(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
    render(mixer, frames).chunks(2).map(|f| f[0]).collect()
}

/// Renders `frames` of interleaved stereo.
pub fn render(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * 2];
    mixer.render(&mut out);
    out
}

pub fn close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!((a - e).abs() < 1e-5, "sample {i}: {a} != {e}\n{actual:?}");
    }
}

pub fn tone_bytes() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/audio/tone-440.ogg"),
    )
    .unwrap()
}
