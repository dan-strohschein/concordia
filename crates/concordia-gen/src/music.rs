//! Music from a score: tracks of notes on a beat grid, each played by a small instrument from
//! the same synthesis pieces as the sound effects, mixed to one mono WAV with loop points.

use std::f32::consts::TAU;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::GenError;
use crate::provider::{ContentProvider, GenKind, Generated};
use crate::wav::{Noise, RATE, encode, normalize};

/// A score: tempo, bars and tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Score {
    /// Beats per minute (20–300).
    pub bpm: f32,
    /// Beats in a bar (1–16).
    pub beats_per_bar: u32,
    /// Bars (1–256).
    pub bars: u32,
    /// Where the loop starts, in bars (default 0: the whole piece loops).
    #[serde(default)]
    pub loop_from_bar: Option<u32>,
    /// The parts.
    pub tracks: Vec<Track>,
}

/// One part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Track {
    /// Its sound.
    pub instrument: Instrument,
    /// Its level (0–1).
    pub volume: f32,
    /// Its notes.
    pub notes: Vec<Note>,
}

/// A note on the beat grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Note {
    /// Start, in beats from the top.
    pub beat: f32,
    /// Length, in beats.
    pub beats: f32,
    /// MIDI pitch (0–127; 60 is middle C).
    pub pitch: u8,
}

/// A small synthesized instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Instrument {
    /// A soft sine.
    Sine,
    /// A bright square.
    Square,
    /// A buzzy saw.
    Saw,
    /// A plucked string (a fast-decaying saw through a low-pass).
    Pluck,
    /// A falling sine thump.
    Kick,
    /// A short noise burst.
    Snare,
    /// A tick of noise.
    Hat,
}

fn check(s: &Score) -> Result<(), GenError> {
    if !(s.bpm.is_finite() && (20.0..=300.0).contains(&s.bpm)) {
        return Err(GenError::invalid(
            "bpm",
            format!("{} is outside 20–300", s.bpm),
        ));
    }
    if !(1..=16).contains(&s.beats_per_bar) {
        return Err(GenError::invalid("beats_per_bar", "must be 1–16"));
    }
    if !(1..=256).contains(&s.bars) {
        return Err(GenError::invalid("bars", "must be 1–256"));
    }
    if s.loop_from_bar.is_some_and(|b| b >= s.bars) {
        return Err(GenError::invalid(
            "loop_from_bar",
            "must be before the last bar's end",
        ));
    }
    for t in &s.tracks {
        if !(t.volume.is_finite() && (0.0..=1.0).contains(&t.volume)) {
            return Err(GenError::invalid("volume", "must be 0–1"));
        }
        for n in &t.notes {
            if n.pitch > 127 {
                return Err(GenError::invalid(
                    "pitch",
                    format!("{} is above 127", n.pitch),
                ));
            }
            if !(n.beat.is_finite() && n.beat >= 0.0 && n.beats.is_finite() && n.beats > 0.0) {
                return Err(GenError::invalid(
                    "beat",
                    "notes start at 0 or later and last a positive number of beats",
                ));
            }
        }
    }
    Ok(())
}

/// One note's samples (`len` of them) for `instrument` at `hz`.
fn voice(instrument: Instrument, hz: f32, len: usize, noise: &mut Noise) -> Vec<f32> {
    let mut low = 0.0f32;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let phase = (hz * t).fract();
            let fade = 1.0 - i as f32 / len as f32;
            match instrument {
                Instrument::Sine => (TAU * phase).sin() * fade.sqrt(),
                Instrument::Square => (if phase < 0.5 { 0.5 } else { -0.5 }) * fade.sqrt(),
                Instrument::Saw => (2.0 * phase - 1.0) * 0.6 * fade.sqrt(),
                Instrument::Pluck => {
                    low += 0.2 * ((2.0 * phase - 1.0) - low);
                    low * (-6.0 * t).exp()
                }
                Instrument::Kick => {
                    (TAU * 50.0 * t * (1.0 + 2.0 * (-20.0 * t).exp())).sin() * (-8.0 * t).exp()
                }
                Instrument::Snare => noise.next() * (-25.0 * t).exp(),
                Instrument::Hat => noise.next() * (-80.0 * t).exp() * 0.5,
            }
        })
        .collect()
}

/// The mixed samples and the loop's (start, end) frames.
pub(crate) fn render(s: &Score, seed: u64) -> Result<(Vec<f32>, u64, u64), GenError> {
    check(s)?;
    let seconds_per_beat = 60.0 / f64::from(s.bpm);
    let frame = |beats: f64| (beats * seconds_per_beat * f64::from(RATE)).round() as u64;
    let total = frame(f64::from(s.bars * s.beats_per_bar));
    let mut mix = vec![0.0f32; total as usize];
    let mut noise = Noise::new(seed);
    for t in &s.tracks {
        for n in &t.notes {
            let start = frame(f64::from(n.beat)) as usize;
            let len = frame(f64::from(n.beats)) as usize;
            let hz = 440.0 * 2f32.powf((f32::from(n.pitch) - 69.0) / 12.0);
            for (i, v) in voice(t.instrument, hz, len, &mut noise)
                .into_iter()
                .enumerate()
            {
                // A note past the end wraps to the top, so the loop is seamless.
                let at = (start + i) % total as usize;
                mix[at] += v * t.volume;
            }
        }
    }
    normalize(&mut mix);
    let loop_start = frame(f64::from(s.loop_from_bar.unwrap_or(0) * s.beats_per_bar));
    Ok((mix, loop_start, total))
}

/// The `music` provider: a [`Score`] as a looping `.wav`.
#[derive(Debug, Clone, Copy, Default)]
pub struct MusicProvider;

impl ContentProvider for MusicProvider {
    fn id(&self) -> &str {
        "music"
    }
    fn kind(&self) -> GenKind {
        GenKind::music
    }
    fn version(&self) -> u32 {
        1
    }
    fn extensions(&self) -> Vec<&str> {
        vec!["wav"]
    }
    fn request_schema(&self) -> Value {
        concordia_reflect::schema_for(<Score as Reflect>::type_info())
    }
    fn generate(
        &self,
        request: &Value,
        seed: u64,
        _: &crate::provider::GenContext<'_>,
    ) -> Result<Generated, GenError> {
        let s: Score = serde_json::from_value(request.clone())
            .map_err(|e| GenError::invalid("request", e.to_string()))?;
        let (samples, loop_start, loop_end) = render(&s, seed)?;
        Ok(Generated {
            bytes: encode(&samples)?,
            info: json!({"frames": samples.len(), "rate": RATE, "loop_start": loop_start, "loop_end": loop_end}),
            settings: Some(json!({"loop_start": loop_start, "loop_end": loop_end})),
            inputs: Vec::new(),
        })
    }
}
