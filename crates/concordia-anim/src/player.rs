//! `AnimPlayer`: plays one clip on a skinned entity at the fixed step.

use concordia_assets::AssetRef;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Plays `clip` on this entity's skeleton (its `Skinned` skin), writing its `Pose` each tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Animation")]
#[serde(deny_unknown_fields)]
pub struct AnimPlayer {
    /// The clip (`asset:<id>#clip/<name>`).
    pub clip: AssetRef,
    /// Playback position in seconds.
    #[serde(default)]
    pub time: f32,
    /// Playback rate (1 = as authored, negative plays backward).
    #[serde(default = "one")]
    pub speed: f32,
    /// Wraps at the end; otherwise holds the last pose.
    #[serde(default = "yes")]
    pub looping: bool,
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

impl AnimPlayer {
    /// Plays `clip` from 0 at normal speed, looping.
    pub fn new(clip: AssetRef) -> Self {
        Self {
            clip,
            time: 0.0,
            speed: one(),
            looping: yes(),
        }
    }
}

/// The playback position after moving `delta` seconds through a clip of `duration`: wrapped
/// when looping, else clamped to the clip; 0 for an empty clip or a non-finite result.
pub fn advance(time: f32, delta: f32, duration: f32, looping: bool) -> f32 {
    let t = time + delta;
    if duration.is_nan() || duration <= 0.0 || !t.is_finite() {
        return 0.0;
    }
    if looping {
        t.rem_euclid(duration)
    } else {
        t.clamp(0.0, duration)
    }
}
