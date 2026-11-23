//! Decoding (hound, lewton) to interleaved f32.
//!
//! Integers scale by 2^(bits−1), so the largest positive sample is just under 1:
//! - 16-bit: [0, 16384, −16384, 32767] / 32768 = [0, 0.5, −0.5, 0.9999695].
//! - 8-bit (hound reads it as signed): [0, 64, −64, 127] / 128 = [0, 0.5, −0.5, 0.9921875].
//! - 24-bit: [0, 2²², −2²², 2²³ − 1] / 2²³ = [0, 0.5, −0.5, 0.99999988].
//! - 32-bit: [0, 2³⁰, −2³⁰, 2³¹ − 1] / 2³¹ = [0, 0.5, −0.5, 1 − 2⁻³¹ ≈ 1.0 in f32].
//! - 32-bit float: as stored.
//!
//! The Ogg fixture (`tools/fixtures/make_audio.sh`) is 1 s of sin(2π·440·t) at amplitude 0.5,
//! mono, 48 kHz. It should decode to exactly 48,000 frames: the last page's granule position
//! trims the final block. A sine's RMS is A/√2 = 0.353553. 440 cycles cross zero 880 times.
//! Vorbis is lossy, so RMS is within 0.01 and crossings within 2.

use std::io::Cursor;
use std::path::Path;

use concordia_audio::{AudioError, Channels, decode_ogg, decode_wav};

fn wav<S: hound::Sample + Copy>(
    channels: u16,
    rate: u32,
    bits: u16,
    format: hound::SampleFormat,
    samples: &[S],
) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: bits,
        sample_format: format,
    };
    let mut bytes = Cursor::new(Vec::new());
    let mut w = hound::WavWriter::new(&mut bytes, spec).unwrap();
    for &s in samples {
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
    bytes.into_inner()
}

fn assert_samples(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() < 1e-7, "{actual:?} != {expected:?}");
    }
}

#[test]
fn wav_decodes_to_expected_frames() {
    use hound::SampleFormat::{Float, Int};
    let d = decode_wav(&wav(1, 48_000, 16, Int, &[0i16, 16384, -16384, 32767])).unwrap();
    assert_eq!(
        (d.rate().get(), d.channels(), d.frames()),
        (48_000, Channels::Mono, 4)
    );
    assert_samples(d.samples(), &[0.0, 0.5, -0.5, 0.999_969_5]);

    let d = decode_wav(&wav(1, 8_000, 8, Int, &[0i8, 64, -64, 127])).unwrap();
    assert_samples(d.samples(), &[0.0, 0.5, -0.5, 0.992_187_5]);

    let d = decode_wav(&wav(
        1,
        48_000,
        24,
        Int,
        &[0i32, 1 << 22, -(1 << 22), (1 << 23) - 1],
    ))
    .unwrap();
    assert_samples(d.samples(), &[0.0, 0.5, -0.5, 0.999_999_9]);

    let d = decode_wav(&wav(
        1,
        48_000,
        32,
        Int,
        &[0i32, 1 << 30, -(1 << 30), i32::MAX],
    ))
    .unwrap();
    assert_samples(d.samples(), &[0.0, 0.5, -0.5, 1.0]);

    let d = decode_wav(&wav(1, 48_000, 32, Float, &[0.0f32, 0.5, -0.5, 1.0])).unwrap();
    assert_samples(d.samples(), &[0.0, 0.5, -0.5, 1.0]);

    let d = decode_wav(&wav(2, 44_100, 16, Int, &[16384i16, -16384, 0, 32767])).unwrap();
    assert_eq!(
        (d.rate().get(), d.channels(), d.frames()),
        (44_100, Channels::Stereo, 2)
    );
    assert_samples(d.samples(), &[0.5, -0.5, 0.0, 0.999_969_5]);

    let three = decode_wav(&wav(3, 48_000, 16, Int, &[0i16, 0, 0])).unwrap_err();
    assert!(three.to_string().contains("3 channels"), "{three}");
    assert!(matches!(
        decode_wav(b"not a wav"),
        Err(AudioError::Decode { format: "wav", .. })
    ));
}

fn fixture(name: &str) -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/audio");
    std::fs::read(root.join(name)).unwrap()
}

#[test]
fn ogg_decodes_to_expected_frames() {
    let d = decode_ogg(&fixture("tone-440.ogg")).unwrap();
    assert_eq!(
        (d.rate().get(), d.channels(), d.frames()),
        (48_000, Channels::Mono, 48_000)
    );
    let s = d.samples();
    let rms = (s.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / s.len() as f64).sqrt();
    assert!((rms - 0.353_553).abs() < 0.01, "rms {rms}");
    let crossings = s
        .windows(2)
        .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
        .count();
    assert!(crossings.abs_diff(880) <= 2, "{crossings} crossings");
    assert!(matches!(
        decode_ogg(b"OggS but not really"),
        Err(AudioError::Decode { format: "ogg", .. })
    ));
}

/// Written after `write_wav16` (moved out of `audio.render`): 16-bit PCM is s × 32768, rounded
/// and clamped: [0.75, −1, 1, 0.5, −0.25] → [24576, −32768, 32767 (clamped from 32768), 16384,
/// −8192], two channels at 48 kHz.
#[test]
fn wav_writer_scales_by_32768_and_clamps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out/x.wav");
    concordia_audio::write_wav16(&path, &[0.75, -1.0, 1.0, 0.5, -0.25, 0.0]).unwrap();
    let wav = hound::WavReader::open(&path).unwrap();
    assert_eq!(
        (
            wav.spec().channels,
            wav.spec().sample_rate,
            wav.spec().bits_per_sample
        ),
        (2, 48_000, 16)
    );
    let samples: Vec<i16> = wav.into_samples::<i16>().map(Result::unwrap).collect();
    assert_eq!(samples, [24576, -32768, 32767, 16384, -8192, 0]);
}
