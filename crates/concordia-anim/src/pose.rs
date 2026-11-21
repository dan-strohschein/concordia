//! `Pose`: every bone's local transform for one character (hashed simulation state, ADR-0074).

use concordia_reflect::Reflect;
use concordia_scene::builtin::Transform;
use serde::{Deserialize, Serialize};

/// Each joint's local transform (relative to its parent bone), in the skin's joint order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Animation", default)]
#[serde(deny_unknown_fields)]
pub struct Pose {
    /// Local transforms, one per joint.
    pub bones: Vec<Transform>,
}
