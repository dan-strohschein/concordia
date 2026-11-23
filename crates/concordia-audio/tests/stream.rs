//! Streaming: a streamed voice reads the same frames as the decoded file, through a ring the
//! offline backend refills before every block, so it never underruns.
//!
//! The 1 s fixture looped for 30 s is 30 × 48,000 frames of the same decoded sequence; streamed
//! and decoded renders must match sample for sample. A one-shot stream ends after 48,000 frames,
//! like its decoded twin.

mod common;

use common::*;
use concordia_audio::{
    LoadedBody, LoadedSound, MASTER, MixerConfig, OfflineBackend, PlayRequest, decode_ogg,
};

fn render(sound: LoadedSound, looping: bool, frames: u64) -> (Vec<f32>, u64, u32) {
    let g = layout(0.0, 0.0, 0.0);
    let mut b = OfflineBackend::new(MixerConfig::new(g.clone())).unwrap();
    b.start_capture();
    b.handle()
        .play(PlayRequest {
            looping,
            ..PlayRequest::new(sound, g.index(MASTER).unwrap())
        })
        .unwrap();
    b.render_frames(frames);
    let underruns = b.handle().stats().underruns();
    let active = b.handle().stats().active();
    (b.take_capture(), underruns, active)
}

fn streamed() -> LoadedSound {
    LoadedSound {
        body: LoadedBody::Stream(tone_bytes().into()),
        gain: 1.0,
        loop_start: 0,
        loop_end: None,
    }
}

fn decoded() -> LoadedSound {
    LoadedSound::memory(decode_ogg(&tone_bytes()).unwrap())
}

#[test]
fn streamed_music_does_not_underrun_offline() {
    let frames = 30 * 48_000;
    let (s, underruns, active) = render(streamed(), true, frames);
    let (d, _, _) = render(decoded(), true, frames);
    assert_eq!(underruns, 0);
    assert_eq!(active, 1);
    assert_eq!(s.len(), frames as usize * 2);
    let first = s
        .iter()
        .zip(&d)
        .position(|(a, b)| a.to_bits() != b.to_bits());
    assert_eq!(first, None, "streamed and decoded differ");
    assert!(s.iter().any(|x| *x != 0.0));
}

#[test]
fn a_one_shot_stream_ends_like_its_decoded_twin() {
    let (s, underruns, active) = render(streamed(), false, 50_000);
    let (d, _, _) = render(decoded(), false, 50_000);
    assert_eq!((underruns, active), (0, 0));
    assert_eq!(s, d);
    assert!(s[96_000..].iter().all(|x| *x == 0.0));
}

#[test]
fn stream_loop_points_wrap_like_memory() {
    let points = |mut s: LoadedSound| {
        s.loop_start = 12_000;
        s.loop_end = Some(36_000);
        s
    };
    let (s, underruns, _) = render(points(streamed()), true, 100_000);
    let (d, _, _) = render(points(decoded()), true, 100_000);
    assert_eq!(underruns, 0);
    assert_eq!(s, d);
}
