//! Writing renders as 16-bit WAV (hound).

use std::path::Path;

/// Writes interleaved stereo at 48 kHz as 16-bit PCM (s × 32768, clamped).
pub fn write_wav16(path: &Path, samples: &[f32]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: crate::mixer::SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for s in samples {
        let v = (s * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        w.write_sample(v).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())
}
