//! `.cue.ron` cues: weighted sounds, volume and pitch ranges, a bus, a priority and a seed; and
//! the `cue` importer. Every play draws from SplitMix64, never a clock or the world RNG.

use concordia_assets::{
    Artifact, AssetError, AssetKind, AssetRef, ImportInput, ImportOutput, Importer, encode,
};
use concordia_reflect::Reflect;
use concordia_terra::splitmix64;
use serde::{Deserialize, Serialize};

use crate::error::CueError;
use crate::gain::semitones_to_ratio;
use crate::spatial::Spatial;

/// An inclusive range.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct Span {
    /// The low end.
    pub min: f32,
    /// The high end.
    pub max: f32,
}

/// One sound a cue may pick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct CueSound {
    /// The imported sound.
    pub sound: AssetRef,
    /// Relative chance (≥ 1).
    #[serde(default = "one")]
    pub weight: u32,
}

fn one() -> u32 {
    1
}

/// A cue: what gameplay plays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(default)]
#[serde(default, deny_unknown_fields)]
pub struct Cue {
    /// The sounds to pick from, by weight (at least one).
    pub sounds: Vec<CueSound>,
    /// Volume range in decibels, each end in [−80, 24].
    pub volume_db: Span,
    /// Pitch range in semitones, each end in [−24, 24].
    pub pitch_semitones: Span,
    /// The bus it plays on.
    pub bus: String,
    /// Higher steals lower when voices run out.
    pub priority: u8,
    /// Loops until stopped.
    pub looping: bool,
    /// The most voices of this cue at once (≥ 1); the oldest stops. None: unlimited.
    pub max_instances: Option<u16>,
    /// Positioned sound: how it fades with distance. None: unpositioned.
    pub spatial: Option<Spatial>,
    /// The variation seed.
    pub seed: u64,
}

impl Default for Cue {
    fn default() -> Self {
        Self {
            sounds: Vec::new(),
            volume_db: Span::default(),
            pitch_semitones: Span::default(),
            bus: "SFX".into(),
            priority: 128,
            looping: false,
            max_instances: None,
            spatial: None,
            seed: 0,
        }
    }
}

/// What a draw depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DrawKey {
    /// The playing entity's bits (0 for previews).
    pub entity: u64,
    /// The tick it starts on.
    pub tick: u64,
    /// Which play from this entity on this tick (0, 1, …).
    pub n: u32,
}

/// One play's variation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Draw {
    /// Index into `sounds`.
    pub sound: usize,
    /// Volume in decibels.
    pub volume_db: f32,
    /// Playback-rate ratio (2^(s/12)).
    pub pitch: f32,
}

impl Cue {
    /// Checks every field, naming the first one at fault.
    pub fn validate(&self) -> Result<(), CueError> {
        if self.sounds.is_empty() {
            return field("sounds", "needs at least one sound".into());
        }
        if let Some(i) = self.sounds.iter().position(|s| s.weight == 0) {
            return field(&format!("sounds[{i}].weight"), "must be ≥ 1".into());
        }
        span("volume_db", self.volume_db, -80.0, 24.0)?;
        span("pitch_semitones", self.pitch_semitones, -24.0, 24.0)?;
        if self.bus.is_empty() {
            return field("bus", "must name a bus".into());
        }
        if self.max_instances == Some(0) {
            return field(
                "max_instances",
                "must be ≥ 1 (or none for unlimited)".into(),
            );
        }
        if let Some(spatial) = &self.spatial {
            spatial
                .validate()
                .or_else(|reason| field("spatial", reason))?;
        }
        Ok(())
    }

    fn total_weight(&self) -> u64 {
        self.sounds.iter().map(|s| u64::from(s.weight)).sum()
    }

    /// The variation for `key`: SplitMix64 from `seed ⊕ entity ⊕ (tick × 2⁸) ⊕ n`, drawing the
    /// sound, then the volume, then the pitch.
    pub fn draw(&self, key: DrawKey) -> Draw {
        let state = self.seed ^ key.entity ^ (key.tick << 8) ^ u64::from(key.n);
        let out = |k: u64| splitmix64(state.wrapping_add(k.wrapping_mul(GAMMA)));
        let pick = out(0) % self.total_weight().max(1);
        let mut acc = 0;
        let sound = self
            .sounds
            .iter()
            .position(|s| {
                acc += u64::from(s.weight);
                pick < acc
            })
            .unwrap_or(0);
        Draw {
            sound,
            volume_db: lerp(self.volume_db, unit(out(1))),
            pitch: semitones_to_ratio(lerp(self.pitch_semitones, unit(out(2)))),
        }
    }
}

/// SplitMix64's increment (the golden ratio's 64-bit fraction).
const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

/// The top 24 bits as a fraction in [0, 1).
fn unit(x: u64) -> f32 {
    (x >> 40) as f32 / (1u32 << 24) as f32
}

fn lerp(span: Span, t: f32) -> f32 {
    span.min + (span.max - span.min) * t
}

fn field(name: &str, reason: String) -> Result<(), CueError> {
    Err(CueError::Field {
        field: name.into(),
        reason,
    })
}

fn span(name: &str, s: Span, low: f32, high: f32) -> Result<(), CueError> {
    for (end, v) in [("min", s.min), ("max", s.max)] {
        if !v.is_finite() || !(low..=high).contains(&v) {
            return field(
                &format!("{name}.{end}"),
                format!("{v} must be in [{low}, {high}]"),
            );
        }
    }
    if s.min > s.max {
        return field(name, format!("min {} is above max {}", s.min, s.max));
    }
    Ok(())
}

/// Reads and validates `.cue.ron` text.
pub fn read_cue(text: &str) -> Result<Cue, CueError> {
    let cue =
        concordia_scene::read_document::<Cue>(text).map_err(|e| CueError::Parse(e.to_string()))?;
    cue.validate()?;
    Ok(cue)
}

/// Writes a cue as canonical `.cue.ron` text.
pub fn write_cue(cue: &Cue) -> Result<String, CueError> {
    cue.validate()?;
    concordia_scene::write_document(cue).map_err(|e| CueError::Parse(e.to_string()))
}

/// The cue importer.
#[derive(Debug, Clone, Copy, Default)]
pub struct CueImporter;

impl Importer for CueImporter {
    fn name(&self) -> &'static str {
        "cue"
    }

    fn version(&self) -> u32 {
        1
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["cue.ron"]
    }

    fn default_settings(&self) -> serde_json::Value {
        serde_json::Value::Object(Default::default())
    }

    fn import(&self, input: ImportInput<'_>) -> Result<ImportOutput, AssetError> {
        let fail = |reason: String| AssetError::Import {
            path: input.path.to_path_buf(),
            reason,
        };
        let text = std::str::from_utf8(input.bytes).map_err(|e| fail(e.to_string()))?;
        let cue = read_cue(text).map_err(|e| fail(e.to_string()))?;
        let mut output = ImportOutput::default();
        output.artifacts.insert(
            String::new(),
            Artifact {
                kind: AssetKind::Cue,
                bytes: encode(&cue)?,
            },
        );
        Ok(output)
    }
}
