//! The offline backend: renders exactly the frames each tick covers, sample-exact, with a running
//! BLAKE3 hash, peak and RMS, and an optional capture.

use std::num::NonZeroU32;
use std::ops::Range;

use crate::error::AudioError;
use crate::mixer::{Mixer, MixerConfig, MixerHandle};

/// The frames tick `tick` covers: [⌊t·rate/tick_rate⌋, ⌊(t+1)·rate/tick_rate⌋).
pub fn tick_frames(tick: u64, tick_rate: NonZeroU32, rate: NonZeroU32) -> Range<u64> {
    let at = |t: u64| {
        let frames = u128::from(t) * u128::from(rate.get()) / u128::from(tick_rate.get());
        u64::try_from(frames).unwrap_or(u64::MAX)
    };
    at(tick)..at(tick.saturating_add(1))
}

/// What has been rendered so far.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderStats {
    /// Frames rendered.
    pub frames: u64,
    /// The largest absolute sample.
    pub peak: f32,
    /// Root mean square over every sample.
    pub rms: f32,
    /// BLAKE3 of every sample's little-endian bytes, hex.
    pub hash: String,
}

/// A mixer rendered on demand, never in real time.
pub struct OfflineBackend {
    mixer: Mixer,
    handle: MixerHandle,
    frames: u64,
    block: Vec<f32>,
    hasher: blake3::Hasher,
    peak: f32,
    sum_sq: f64,
    capture: Option<Vec<f32>>,
}

impl std::fmt::Debug for OfflineBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OfflineBackend")
            .field("frames", &self.frames)
            .field("mixer", &self.mixer)
            .finish()
    }
}

impl OfflineBackend {
    /// A backend over a new mixer.
    pub fn new(config: MixerConfig) -> Result<Self, AudioError> {
        let block = vec![0.0; config.max_block.get() * 2];
        let (mixer, handle) = Mixer::new(config)?;
        Ok(Self {
            mixer,
            handle,
            frames: 0,
            block,
            hasher: blake3::Hasher::new(),
            peak: 0.0,
            sum_sq: 0.0,
            capture: None,
        })
    }

    /// The engine half, for queuing commands.
    pub fn handle(&mut self) -> &mut MixerHandle {
        &mut self.handle
    }

    /// Renders `frames` more frames.
    /// Streams are refilled before every block, so offline renders never underrun. Returns
    /// stream decode errors (those streams end).
    pub fn render_frames(&mut self, frames: u64) -> Vec<AudioError> {
        let mut errors = Vec::new();
        let mut left = frames;
        let per_block = (self.block.len() / 2) as u64;
        while left > 0 {
            let n = left.min(per_block) as usize;
            errors.extend(self.handle.pump_streams());
            let block = &mut self.block[..n * 2];
            self.mixer.render(block);
            self.hasher.update(&samples_le(block));
            for s in block.iter() {
                self.peak = self.peak.max(s.abs());
                self.sum_sq += f64::from(*s) * f64::from(*s);
            }
            if let Some(c) = self.capture.as_mut() {
                c.extend_from_slice(block);
            }
            self.handle.collect();
            self.frames += n as u64;
            left -= n as u64;
        }
        errors
    }

    /// Renders up to the end of `tick` (nothing if it is already rendered).
    pub fn render_tick(&mut self, tick: u64, tick_rate: NonZeroU32) -> Vec<AudioError> {
        let end = tick_frames(tick, tick_rate, self.handle.rate()).end;
        self.render_frames(end.saturating_sub(self.frames))
    }

    /// Frames rendered so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The running statistics.
    pub fn stats(&self) -> RenderStats {
        let samples = self.frames * 2;
        let rms = if samples == 0 {
            0.0
        } else {
            (self.sum_sq / samples as f64).sqrt() as f32
        };
        RenderStats {
            frames: self.frames,
            peak: self.peak,
            rms,
            hash: self.hasher.finalize().to_hex().to_string(),
        }
    }

    /// Starts keeping every rendered sample.
    pub fn start_capture(&mut self) {
        self.capture = Some(Vec::new());
    }

    /// The samples kept since `start_capture` (interleaved stereo); keeps capturing.
    pub fn take_capture(&mut self) -> Vec<f32> {
        self.capture
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }
}

fn samples_le(block: &[f32]) -> Vec<u8> {
    block.iter().flat_map(|s| s.to_le_bytes()).collect()
}
