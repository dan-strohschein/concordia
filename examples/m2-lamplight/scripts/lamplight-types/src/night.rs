//! The night's broadcast: what every module may know about the night, raised by the rules
//! module on the night entity each tick.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Before the night starts (the title screen).
pub const TITLE: &str = "title";
/// The night is on.
pub const PLAYING: &str = "playing";
/// Paused.
pub const PAUSED: &str = "paused";
/// Dawn came with spirit left.
pub const WON: &str = "won";
/// Town Spirit ran out.
pub const LOST: &str = "lost";

/// Signal: the night this tick (raised on the night entity, delivered next tick).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.NightTick", default)]
pub struct NightTick {
    /// `title`, `playing`, `paused`, `won` or `lost`.
    pub phase: String,
    /// Seconds of night played.
    pub clock: f32,
    /// The night's length in seconds.
    pub length: f32,
    /// Town Spirit left.
    pub spirit: u32,
    /// How thick the fog is: wave sizes scale by it (1 is normal).
    pub density: f32,
}

impl NightTick {
    /// Whether the night is being played.
    pub fn playing(&self) -> bool {
        self.phase == PLAYING
    }
}
