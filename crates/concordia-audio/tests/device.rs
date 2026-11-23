//! The `cpal` backend on a real device (manual: `cargo nextest run -p concordia-audio --run-ignored
//! only`, on a machine with speakers). It plays a quiet 0.3 s 440 Hz tone (−30 dBFS) and checks
//! the device's callback rendered it to the end: the voice is retired within a second.

use std::num::{NonZeroU32, NonZeroUsize};
use std::time::{Duration, Instant};

use concordia_audio::{
    AudioDevice, BusGraph, BusLayout, Channels, CpalDevice, LoadedSound, MASTER, PlayRequest,
    SoundData,
};

#[test]
#[ignore = "manual: needs an audio output device and plays a sound"]
fn cpal_device_plays_a_cue() {
    let buses = BusGraph::new(&BusLayout::default()).unwrap();
    let mut open = CpalDevice
        .open(&buses, NonZeroUsize::new(8).unwrap())
        .expect("a default output device");
    let rate = 48_000u32;
    let tone: Vec<f32> = (0..rate * 3 / 10)
        .map(|i| 0.0316 * (std::f32::consts::TAU * 440.0 * i as f32 / rate as f32).sin())
        .collect();
    let sound = LoadedSound::memory(
        SoundData::new(NonZeroU32::new(rate).unwrap(), Channels::Mono, tone).unwrap(),
    );
    let voice = open
        .handle
        .play(PlayRequest::new(sound, buses.index(MASTER).unwrap()))
        .unwrap();
    let started = Instant::now();
    while open.handle.is_live(voice) {
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the device never finished the tone"
        );
        std::thread::sleep(Duration::from_millis(20));
        open.handle.collect();
    }
    assert!(
        started.elapsed() >= Duration::from_millis(250),
        "it played in real time"
    );
    drop(open.guard);
}
