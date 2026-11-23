//! Walkers and their animated bodies: a walker (the entity that moves) drives a skinned body
//! under it through `Stride` signals. The rules module owns these types; the fog module's
//! townsfolk use them too.

use concordia_reflect::Reflect;
use concordia_script_guest::EntityId;
use serde::{Deserialize, Serialize};

/// An animated body: the skinned entity a walker (its `walker`) moves and animates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Body", default)]
pub struct Body {
    /// The entity that walks it.
    pub walker: Option<EntityId>,
}

/// Signal: how a body should move this tick (from its walker, on the body).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "lamplight.Stride", default)]
pub struct Stride {
    /// The graph's `walk_amount` (0..1).
    pub walk_amount: f32,
    /// Fire the Light one-shot.
    pub light: bool,
    /// Fire the Startle one-shot.
    pub startle: bool,
}
