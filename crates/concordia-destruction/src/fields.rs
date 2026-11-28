//! Fields (M8 8.4): a region, placed by its entity's `Transform`, that acts on fragments.

use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_scene::builtin::Transform;
use serde::{Deserialize, Serialize};

/// A region of space that acts on breakables in play. An ordinary component: edit and undo it
/// like any other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Physics")]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// Where it acts, around its entity's position.
    pub shape: FieldShape,
    /// How its effect fades away from its centre.
    pub falloff: Falloff,
    /// What it does.
    pub effect: FieldEffect,
    /// How strongly: strain per second for `Strain`; unused by the others.
    #[reflect(range(min = 0.0, max = 1000000000.0))]
    pub magnitude: f32,
    /// Seconds it acts in play (0: for ever); it is removed from the play world when it ends.
    #[reflect(range(min = 0.0, max = 1000000.0))]
    pub duration: f32,
}

/// A field's region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum FieldShape {
    /// A ball.
    Sphere {
        /// Its radius (m).
        radius: f32,
    },
    /// A box aligned with the entity's rotation.
    Box {
        /// Half its size per axis (m).
        half_extents: Vec3,
    },
    /// Everything on the back side of a plane through the entity.
    Plane {
        /// The plane's normal, in the entity's space (the field is where `normal · p ≤ 0`).
        normal: Vec3,
    },
    /// Everywhere.
    Everywhere,
}

/// How an effect fades with distance from the field's centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Falloff {
    /// Full strength everywhere inside.
    None,
    /// From full at the centre to nothing at the shape's reach (sphere radius, box half
    /// diagonal; planes and `Everywhere` have no reach, so they don't fade).
    Linear,
    /// 1 / (1 + d²), d in metres.
    InverseSquare,
}

/// What a field does to the fragments it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum FieldEffect {
    /// Damages the bonds inside it by `magnitude` per second.
    Strain,
    /// Breaks every bond of each fragment with a piece inside it.
    Crumble,
    /// Holds the pieces inside it in place: their fragments become static.
    Anchor,
    /// Removes debris (parts already split off) whose centre of mass is inside it.
    Kill,
}

/// How long a field has acted in play.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Physics", default)]
pub struct FieldAge {
    /// Seconds.
    pub seconds: f32,
}

impl Field {
    /// The effect's weight at world point `p` (0 outside the shape), for a field at `at`.
    pub fn weight(&self, at: &Transform, p: Vec3) -> f32 {
        let local = at.rotation.inverse() * (p - at.translation);
        let d = local.length();
        let (inside, reach) = match &self.shape {
            FieldShape::Sphere { radius } => (d <= *radius, Some(*radius)),
            FieldShape::Box { half_extents } => (
                local.abs().cmple(*half_extents).all(),
                Some(half_extents.length()),
            ),
            FieldShape::Plane { normal } => (normal.dot(local) <= 0.0, None),
            FieldShape::Everywhere => (true, None),
        };
        if !inside {
            return 0.0;
        }
        match (self.falloff, reach) {
            (Falloff::Linear, Some(r)) if r > 0.0 => (1.0 - d / r).max(0.0),
            (Falloff::InverseSquare, _) => 1.0 / (1.0 + d * d),
            _ => 1.0,
        }
    }
}
