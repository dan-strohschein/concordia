//! Gameplay's audio requests: hashed simulation state (ADR-0066). Voices and the mixer live
//! outside the world.

use std::collections::BTreeMap;

use concordia_assets::AssetRef;
use concordia_reflect::{Reflect, RegistryError, TypeRegistry};
use concordia_terra::register_component;
use serde::{Deserialize, Serialize};

/// Plays a cue while `playing` is true: a looping cue loops; a one-shot cue plays once each
/// time `playing` turns true. Positioned at the entity's `Transform` when the cue is spatial.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Audio")]
#[serde(deny_unknown_fields)]
pub struct AudioEmitter {
    /// The `.cue.ron` cue.
    pub cue: AssetRef,
    /// Sounding (default true).
    #[serde(default = "yes")]
    pub playing: bool,
    /// Extra volume in decibels, in [−80, 24].
    #[serde(default)]
    #[reflect(range(min = -80.0, max = 24.0))]
    pub volume_db: f32,
}

fn yes() -> bool {
    true
}

/// Where sound is heard from: the lowest-id entity with one (its `Transform`: right +x,
/// forward −z). Put it on the camera or the player.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Audio", default)]
#[serde(default, deny_unknown_fields)]
pub struct AudioListener {
    /// Scales every positioned sound's gain (default 1).
    #[reflect(range(min = 0.0, max = 16.0))]
    pub gain: f32,
}

impl Default for AudioListener {
    fn default() -> Self {
        Self { gain: 1.0 }
    }
}

/// Animation clip events that sound: event name → cue, played at the entity (2.4.4's
/// `AnimEvent`). Events with no entry are ignored.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Audio", default)]
#[serde(default, deny_unknown_fields)]
pub struct AnimEventCues {
    /// Event name → cue.
    pub cues: BTreeMap<String, AssetRef>,
}

/// A signal: play a cue once at the target entity (unpositioned if it has no `Transform`).
/// Raised on tick t, it sounds on tick t + 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Audio")]
#[serde(deny_unknown_fields)]
pub struct PlayCue {
    /// The cue.
    pub cue: AssetRef,
    /// Extra volume in decibels.
    #[serde(default)]
    pub volume_db: f32,
}

/// A signal: stop what the target entity started (that cue only, or everything), with the
/// 5 ms ramp.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Audio", default)]
#[serde(default, deny_unknown_fields)]
pub struct StopCue {
    /// Only this cue (none: all of them).
    pub cue: Option<AssetRef>,
}

/// Registers the audio components and signals, plus `Cue` and `BusLayout` (for `schema.get`).
pub fn register_types(registry: &mut TypeRegistry) -> Result<(), RegistryError> {
    register_component::<AudioEmitter>(registry)?;
    register_component::<AudioListener>(registry)?;
    register_component::<AnimEventCues>(registry)?;
    register_component::<PlayCue>(registry)?;
    register_component::<StopCue>(registry)?;
    register_component::<SetBusGain>(registry)?;
    registry.register::<crate::Cue>()?;
    registry.register::<crate::BusLayout>()?;
    Ok(())
}

/// A signal: set bus `bus`'s gain to `gain_db` (in [−80, 24]) from gameplay, as `audio.buses`
/// does; applied the tick it is delivered and kept until the engine closes. An unknown bus or a
/// gain out of range is warned about once and ignored.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Audio", default)]
#[serde(default, deny_unknown_fields)]
pub struct SetBusGain {
    /// The bus.
    pub bus: String,
    /// The gain in decibels.
    pub gain_db: f32,
}
