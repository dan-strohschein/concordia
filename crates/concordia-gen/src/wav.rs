//! Mono 16-bit 48 kHz WAV output, normalized to a peak, and a seeded noise source.

use crate::error::GenError;

/// The rate every generated sound uses.
pub(crate) const RATE: u32 = 48_000;

/// −1 dBFS as a linear peak.
pub(crate) const PEAK: f32 = 0.891_250_9;

/// Scales `samples` so the loudest is at [`PEAK`] (silence stays silent).
pub(crate) fn normalize(samples: &mut [f32]) {
    let loudest = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if loudest > 0.0 {
        let gain = PEAK / loudest;
        samples.iter_mut().for_each(|s| *s *= gain);
    }
}

/// `samples` as a mono 16-bit WAV at [`RATE`].
pub(crate) fn encode(samples: &[f32]) -> Result<Vec<u8>, GenError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut out = std::io::Cursor::new(Vec::new());
    let mut writer =
        hound::WavWriter::new(&mut out, spec).map_err(|e| GenError::Output(e.to_string()))?;
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        writer
            .write_sample(v)
            .map_err(|e| GenError::Output(e.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|e| GenError::Output(e.to_string()))?;
    Ok(out.into_inner())
}

/// A seeded xorshift noise source in [-1, 1].
pub(crate) struct Noise(u64);

impl Noise {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub(crate) fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 40) as f32 / (1u64 << 23) as f32 - 1.0
    }
}
