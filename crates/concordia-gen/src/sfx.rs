//! Sound effects from a parametric synthesizer (in the spirit of sfxr's public design notes):
//! an oscillator or noise, an exponential pitch sweep, an attack-decay envelope and a one-pole
//! low-pass, normalized to −1 dBFS.

use std::f32::consts::TAU;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::GenError;
use crate::provider::{ContentProvider, GenKind, Generated};
use crate::wav::{Noise, RATE, encode, normalize};

/// A starting recipe.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum SfxPreset {
    /// A bright rising blip.
    pickup,
    /// A short noisy thud.
    hit,
    /// A rising square sweep.
    jump,
    /// A long falling noise burst.
    explosion,
    /// A tiny click.
    ui_click,
}

/// A sound effect: a preset and optional changes to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SfxRequest {
    /// The recipe.
    pub preset: SfxPreset,
    /// Length in milliseconds (1–5000; default the preset's).
    #[serde(default)]
    pub duration_ms: Option<u32>,
    /// End pitch over start pitch (0.1–10; default the preset's).
    #[serde(default)]
    pub pitch_sweep: Option<f32>,
    /// Start pitch in Hz (20–8000; default the preset's).
    #[serde(default)]
    pub base_hz: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
enum Wave {
    Square,
    Sine,
    Noise,
}

/// A preset's defaults: wave, length, start pitch, sweep, attack fraction, low-pass (0–1).
fn recipe(p: SfxPreset) -> (Wave, u32, f32, f32, f32, f32) {
    match p {
        SfxPreset::pickup => (Wave::Square, 180, 880.0, 1.6, 0.02, 0.9),
        SfxPreset::hit => (Wave::Noise, 120, 200.0, 0.5, 0.01, 0.35),
        SfxPreset::jump => (Wave::Square, 260, 300.0, 1.8, 0.05, 0.8),
        SfxPreset::explosion => (Wave::Noise, 700, 120.0, 0.3, 0.01, 0.2),
        SfxPreset::ui_click => (Wave::Sine, 40, 1600.0, 0.9, 0.05, 1.0),
    }
}

fn in_range(field: &str, v: f32, lo: f32, hi: f32) -> Result<f32, GenError> {
    if v.is_finite() && (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(GenError::invalid(
            field,
            format!("{v} is outside {lo}–{hi}"),
        ))
    }
}

/// The samples of `r` (seeded noise for noisy presets).
pub(crate) fn synth(r: &SfxRequest, seed: u64) -> Result<Vec<f32>, GenError> {
    let (wave, ms, hz, sweep, attack, lowpass) = recipe(r.preset);
    let ms = r.duration_ms.unwrap_or(ms);
    if !(1..=5000).contains(&ms) {
        return Err(GenError::invalid(
            "duration_ms",
            format!("{ms} is outside 1–5000"),
        ));
    }
    let sweep = in_range("pitch_sweep", r.pitch_sweep.unwrap_or(sweep), 0.1, 10.0)?;
    let hz = in_range("base_hz", r.base_hz.unwrap_or(hz), 20.0, 8000.0)?;
    let n = (RATE as u64 * u64::from(ms) / 1000) as usize;
    let mut noise = Noise::new(seed);
    let (mut phase, mut held, mut low) = (0.0f32, 0.0f32, 0.0f32);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / n as f32;
        let f = hz * sweep.powf(t);
        let before = phase;
        phase = (phase + f / RATE as f32).fract();
        let raw = match wave {
            Wave::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Wave::Sine => (TAU * phase).sin(),
            Wave::Noise => {
                if phase < before {
                    held = noise.next();
                }
                held
            }
        };
        low += lowpass * (raw - low);
        let envelope = if t < attack {
            t / attack
        } else {
            1.0 - (t - attack) / (1.0 - attack)
        };
        out.push(low * envelope);
    }
    normalize(&mut out);
    Ok(out)
}

/// The `sfx` provider: a [`SfxRequest`] as a `.wav`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SfxProvider;

impl ContentProvider for SfxProvider {
    fn id(&self) -> &'static str {
        "sfx"
    }
    fn kind(&self) -> GenKind {
        GenKind::sfx
    }
    fn version(&self) -> u32 {
        1
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["wav"]
    }
    fn request_schema(&self) -> Value {
        concordia_reflect::schema_for(<SfxRequest as Reflect>::type_info())
    }
    fn generate(&self, request: &Value, seed: u64) -> Result<Generated, GenError> {
        let r: SfxRequest = serde_json::from_value(request.clone())
            .map_err(|e| GenError::invalid("request", e.to_string()))?;
        let samples = synth(&r, seed)?;
        Ok(Generated {
            bytes: encode(&samples)?,
            info: json!({"frames": samples.len(), "rate": RATE, "seconds": samples.len() as f64 / f64::from(RATE)}),
            settings: None,
        })
    }
}
