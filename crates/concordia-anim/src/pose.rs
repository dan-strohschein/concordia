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
            .map(|(a, b)| blend_bone(a, b, alpha))
            .collect();
        Ok(Pose { bones })
    }

    /// Moves each bone `weight` of the way to `to`'s (only bones `mask` marks, if given).
    pub(crate) fn blend_toward(&mut self, to: &Pose, weight: f32, mask: Option<&[bool]>) {
        debug_assert_eq!(self.bones.len(), to.bones.len(), "poses of one skeleton");
        for (j, (a, b)) in self.bones.iter_mut().zip(&to.bones).enumerate() {
            if mask.is_none_or(|m| m.get(j).copied().unwrap_or(false)) {
                *a = blend_bone(a, b, weight);
            }
        }
    }
}

/// One bone `w` of the way from `a` to `b`: lerp, shortest-path slerp, lerp.
fn blend_bone(a: &Transform, b: &Transform, w: f32) -> Transform {
    Transform {
        translation: a.translation.lerp(b.translation, w),
        rotation: a.rotation.slerp(b.rotation, w),
        scale: a.scale.lerp(b.scale, w),
    }
}
