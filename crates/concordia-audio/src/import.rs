//! The `sound` importer: `.wav` and `.ogg` → `Sound` artifacts in the DDC.

use concordia_assets::{
    Artifact, AssetError, AssetKind, ImportInput, ImportOutput, Importer, encode,
};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::decode::{decode_ogg, decode_wav};
use crate::sound::{Sound, SoundBody};

/// Ogg longer than this streams unless the sidecar says otherwise.
pub const STREAM_AFTER_SECONDS: u32 = 10;

/// Sound import settings (in the sidecar).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct SoundSettings {
    /// Stream while playing (Ogg only); none: Ogg longer than 10 s streams.
    pub stream: Option<bool>,
    /// Import gain in decibels.
    #[reflect(range(min = -80.0, max = 24.0))]
    pub gain_db: f32,
    /// Where a loop restarts, in frames.
    pub loop_start: u64,
    /// Where a loop wraps (exclusive), in frames; none is the end.
    pub loop_end: Option<u64>,
}

impl SoundSettings {
    /// Whether a source of `frames` at `rate` streams (only Ogg can).
    pub fn streams(&self, is_ogg: bool, frames: u64, rate: u32) -> bool {
        is_ogg
            && self
                .stream
                .unwrap_or(frames > u64::from(rate) * u64::from(STREAM_AFTER_SECONDS))
    }

    /// Checks the gain and that the loop points fall inside `frames`.
    fn check(&self, frames: u64) -> Result<(), String> {
        if !self.gain_db.is_finite() || !(-80.0..=24.0).contains(&self.gain_db) {
            return Err(format!("gain_db {} must be in [-80, 24]", self.gain_db));
        }
        let end = self.loop_end.unwrap_or(frames);
        if end > frames {
            return Err(format!(
                "loop_end {end} is past the sound's {frames} frames"
            ));
        }
        if self.loop_start >= end {
            return Err(format!(
                "loop_start {} must be before loop_end {end}",
                self.loop_start
            ));
        }
        Ok(())
    }
}

/// The sound importer.
#[derive(Debug, Clone, Copy, Default)]
pub struct SoundImporter;

impl Importer for SoundImporter {
    fn name(&self) -> &'static str {
        "sound"
    }

    fn version(&self) -> u32 {
        1
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["wav", "ogg"]
    }

    fn default_settings(&self) -> serde_json::Value {
        serde_json::to_value(SoundSettings::default()).unwrap_or_default()
    }

    fn import(&self, input: ImportInput<'_>) -> Result<ImportOutput, AssetError> {
        let fail = |reason: String| AssetError::Import {
            path: input.path.to_path_buf(),
            reason,
        };
        let settings: SoundSettings = serde_json::from_value(input.settings.clone())
            .map_err(|e| fail(format!("settings: {e}")))?;
        let is_ogg = input
            .path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ogg"));
        let data = if is_ogg {
            decode_ogg(input.bytes)
        } else {
            decode_wav(input.bytes)
        }
        .map_err(|e| fail(e.to_string()))?;
        let frames = data.frames() as u64;
        settings.check(frames).map_err(fail)?;
        let body = if settings.streams(is_ogg, frames, data.rate().get()) {
            SoundBody::Streamed(input.bytes.to_vec())
        } else {
            SoundBody::Decoded(data)
        };
        let sound = Sound {
            body,
            gain_db: settings.gain_db,
            loop_start: settings.loop_start,
            loop_end: settings.loop_end,
        };
        let mut output = ImportOutput::default();
        output.artifacts.insert(
            String::new(),
            Artifact {
                kind: AssetKind::Sound,
                bytes: encode(&sound)?,
            },
        );
        Ok(output)
    }
}
