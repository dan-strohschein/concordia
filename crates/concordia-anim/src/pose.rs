//! `Pose`: every bone's local transform for one character (hashed simulation state, ADR-0074).

use concordia_reflect::Reflect;

use crate::error::AnimError;
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

impl Pose {
    /// The pose `alpha` of the way from `self` to `to`, bone by bone (lerp, slerp, lerp).
    pub fn lerp(&self, to: &Pose, alpha: f32) -> Result<Pose, AnimError> {
        if self.bones.len() != to.bones.len() {
            return Err(AnimError::PoseSize {
                expected: self.bones.len(),
                got: to.bones.len(),
            });
        }
        let bones = self
            .bones
            .iter()
            .zip(&to.bones)
            .map(|(a, b)| Transform {
                translation: a.translation.lerp(b.translation, alpha),
                rotation: a.rotation.slerp(b.rotation, alpha),
                scale: a.scale.lerp(b.scale, alpha),
            })
            .collect();
        Ok(Pose { bones })
    }
}
