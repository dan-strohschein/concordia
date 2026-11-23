//! Decoded sample data, the `Sound` artifact and its loaded, shareable form.

use std::num::NonZeroU32;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::AudioError;
use crate::gain::db_to_gain;

/// Mono or stereo (more is an import error).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channels {
    /// One channel.
    Mono,
    /// Two channels, interleaved left then right.
    Stereo,
}

impl Channels {
    /// From a channel count (1 or 2).
    pub fn from_count(count: u16) -> Result<Self, AudioError> {
        match count {
            1 => Ok(Self::Mono),
            2 => Ok(Self::Stereo),
            n => Err(AudioError::Sound(format!(
                "{n} channels; only mono and stereo are supported"
            ))),
        }
    }

    /// The channel count.
    pub fn count(self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }
}

/// Interleaved `f32` samples at a rate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundData {
    rate: NonZeroU32,
    channels: Channels,
    samples: Vec<f32>,
}

impl SoundData {
    /// Checks that `samples` holds whole, finite frames.
    pub fn new(
        rate: NonZeroU32,
        channels: Channels,
        samples: Vec<f32>,
    ) -> Result<Self, AudioError> {
        if !samples.len().is_multiple_of(channels.count()) {
            return Err(AudioError::Sound(format!(
                "{} samples is not a whole number of {}-channel frames",
                samples.len(),
                channels.count()
            )));
        }
        if let Some(i) = samples.iter().position(|s| !s.is_finite()) {
            return Err(AudioError::Sound(format!("sample {i} is not finite")));
        }
        Ok(Self {
            rate,
            channels,
            samples,
        })
    }

    /// Frames per second.
    pub fn rate(&self) -> NonZeroU32 {
        self.rate
    }

    /// Mono or stereo.
    pub fn channels(&self) -> Channels {
        self.channels
    }

    /// Interleaved samples.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The number of frames.
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.count()
    }
}

/// How a sound's samples are kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SoundBody {
    /// Decoded in the DDC.
    Decoded(SoundData),
    /// Ogg Vorbis bytes, decoded while playing.
    Streamed(Vec<u8>),
}

/// The `sound` importer's artifact (`AssetKind::Sound`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sound {
    /// The samples or the stream.
    pub body: SoundBody,
    /// Import gain in decibels.
    pub gain_db: f32,
    /// Where a loop restarts, in frames.
    pub loop_start: u64,
    /// Where a loop wraps (exclusive), in frames; none is the end.
    pub loop_end: Option<u64>,
}

/// A sound ready to play: its body shared, so requests clone cheaply.
#[derive(Debug, Clone)]
pub struct LoadedSound {
    /// The samples or the stream.
    pub body: LoadedBody,
    /// Linear import gain.
    pub gain: f32,
    /// Where a loop restarts, in frames.
    pub loop_start: u64,
    /// Where a loop wraps (exclusive), in frames; none is the end.
    pub loop_end: Option<u64>,
}

/// A loaded sound's samples or stream.
#[derive(Debug, Clone)]
pub enum LoadedBody {
    /// Decoded samples.
    Memory(Arc<SoundData>),
    /// Ogg Vorbis bytes.
    Stream(Arc<[u8]>),
}

impl From<Sound> for LoadedSound {
    fn from(sound: Sound) -> Self {
        Self {
            body: match sound.body {
                SoundBody::Decoded(data) => LoadedBody::Memory(Arc::new(data)),
                SoundBody::Streamed(bytes) => LoadedBody::Stream(bytes.into()),
            },
            gain: db_to_gain(sound.gain_db),
            loop_start: sound.loop_start,
            loop_end: sound.loop_end,
        }
    }
}

impl LoadedSound {
    /// Decoded samples at unit gain with no loop points.
    pub fn memory(data: SoundData) -> Self {
        Self {
            body: LoadedBody::Memory(Arc::new(data)),
            gain: 1.0,
            loop_start: 0,
            loop_end: None,
        }
    }
}
