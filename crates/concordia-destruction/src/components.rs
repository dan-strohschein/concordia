//! The authored `Breakable` and the runtime `Fragment`.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Breaks under impacts in play: the entity (with a `Transform` and a `RigidBody`) becomes one
/// body made of the recipe's pieces, which splits as its bonds break.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Physics")]
#[serde(deny_unknown_fields)]
pub struct Breakable {
    /// Project-relative `.breakable.ron`.
    pub recipe: String,
    /// Cells held in place: parts holding one stay static.
    pub anchors: Vec<u32>,
    /// How far from an impact bonds take damage (metres; damage falls off linearly).
    #[reflect(range(min = 0.0, max = 1000.0))]
    pub impact_radius: f32,
}

/// One body of a breakable in play: which pieces it holds and the damage its bonds have taken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Physics")]
#[serde(deny_unknown_fields)]
pub struct Fragment {
    /// The breakable's recipe.
    pub recipe: String,
    /// Cells held in place.
    pub anchors: Vec<u32>,
    /// How far from an impact bonds take damage.
    pub impact_radius: f32,
    /// Its pieces (indices into the recipe's pieces), ascending.
    pub pieces: Vec<u32>,
    /// Damage per bond of the structure (only its own bonds change).
    pub damage: Vec<f32>,
    /// Bonds that broke, ascending.
    pub broken: Vec<u32>,
}
