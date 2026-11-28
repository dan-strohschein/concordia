//! Reflected physics components (scene data).

use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::layers::PhysicsLayer;

/// How a body moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum BodyKind {
    /// Never moves (ground, walls).
    Static,
    /// Moved by the simulation.
    #[default]
    Dynamic,
    /// Follows its `Transform`, pushing dynamic bodies.
    Kinematic,
}

/// A rigid body; needs a `Transform` and a `Collider` on the same entity (root entities only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Physics", default)]
#[serde(default, deny_unknown_fields)]
pub struct RigidBody {
    /// Static, dynamic or kinematic.
    pub kind: BodyKind,
    /// Mass in kg (default: from the shape at 1000 kg/m³).
    #[reflect(range(min = 0.0, max = 1000000.0))]
    pub mass: Option<f32>,
    /// Friction coefficient.
    #[reflect(range(min = 0.0, max = 10.0))]
    pub friction: f32,
    /// Bounciness (0 = none, 1 = elastic).
    #[reflect(range(min = 0.0, max = 1.0))]
    pub restitution: f32,
    /// Linear velocity damping per second.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub linear_damping: f32,
    /// Angular velocity damping per second.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub angular_damping: f32,
    /// Multiplier on world gravity.
    #[reflect(range(min = -100.0, max = 100.0))]
    pub gravity_scale: f32,
    /// Continuous collision detection for fast bodies.
    pub ccd: bool,
}

impl Default for RigidBody {
    fn default() -> Self {
        Self {
            kind: BodyKind::Dynamic,
            mass: None,
            friction: 0.5,
            restitution: 0.0,
            linear_damping: 0.05,
            angular_damping: 0.05,
            gravity_scale: 1.0,
            ccd: false,
        }
    }
}

impl RigidBody {
    /// A dynamic body with defaults.
    pub fn dynamic() -> Self {
        Self::default()
    }

    /// A static body with defaults.
    pub fn fixed() -> Self {
        Self {
            kind: BodyKind::Static,
            ..Self::default()
        }
    }
}

/// A collision shape, centered on the entity (plus the collider's offset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum ColliderShape {
    /// A box.
    Box {
        /// Half the size along each axis, in meters.
        half_extents: Vec3,
    },
    /// A sphere.
    Sphere {
        /// Radius in meters.
        radius: f32,
    },
    /// A Y-aligned capsule.
    Capsule {
        /// Half the height of the cylindrical part.
        half_height: f32,
        /// Radius.
        radius: f32,
    },
    /// A Y-aligned cylinder.
    Cylinder {
        /// Half the height.
        half_height: f32,
        /// Radius.
        radius: f32,
    },
    /// The convex hull of points in body space (at least 4, not all in a plane; M8 8.0a).
    ConvexHull {
        /// The points.
        points: Vec<Vec3>,
    },
    /// Several convex hulls as one body (a fractured piece's cluster; M8 8.0a).
    Compound {
        /// Each part's points, in body space.
        parts: Vec<Vec<Vec3>>,
    },
}

/// The shape a rigid body collides with. `Transform.scale` does not scale it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Physics", default)]
#[serde(deny_unknown_fields)]
pub struct Collider {
    /// Geometry.
    pub shape: ColliderShape,
    /// Collision layer (0..=15).
    #[serde(default)]
    #[reflect(range(min = 0.0, max = 15.0))]
    pub layer: PhysicsLayer,
    /// Detects overlaps (contact signals) without pushing anything.
    #[serde(default)]
    pub is_sensor: bool,
    /// Shape offset from the entity's origin, in local space.
    #[serde(default)]
    pub offset: Vec3,
}

impl Default for Collider {
    /// A solid 1 m box.
    fn default() -> Self {
        Self::new(ColliderShape::Box {
            half_extents: Vec3::splat(0.5),
        })
    }
}

impl Collider {
    /// A solid collider on layer 0 with no offset.
    pub fn new(shape: ColliderShape) -> Self {
        Self {
            shape,
            layer: PhysicsLayer(0),
            is_sensor: false,
            offset: Vec3::ZERO,
        }
    }
}

/// A walking character (Jolt CharacterVirtual): moved by `CharacterMotion`, feet at `Transform.translation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Physics", default)]
#[serde(default, deny_unknown_fields)]
pub struct CharacterController {
    /// Total capsule height in meters.
    #[reflect(range(min = 0.1, max = 100.0))]
    pub height: f32,
    /// Capsule radius in meters.
    #[reflect(range(min = 0.01, max = 50.0))]
    pub radius: f32,
    /// Steepest walkable slope in degrees.
    #[reflect(range(min = 1.0, max = 89.0))]
    pub max_slope_deg: f32,
    /// Tallest step it climbs, in meters.
    #[reflect(range(min = 0.0, max = 10.0))]
    pub step_height: f32,
    /// Collision layer (0..=15).
    #[reflect(range(min = 0.0, max = 15.0))]
    pub layer: PhysicsLayer,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            height: 1.8,
            radius: 0.3,
            max_slope_deg: 45.0,
            step_height: 0.3,
            layer: PhysicsLayer(0),
        }
    }
}

/// Desired horizontal velocity of a character this tick (gameplay writes it; y is ignored).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Physics", default)]
#[serde(default, deny_unknown_fields)]
pub struct CharacterMotion {
    /// Meters per second.
    pub velocity: Vec3,
}

/// Asks for [`crate::ContactImpact`] signals on this entity for contacts that begin at least
/// `min_strength` hard (N·s; M8 8.0b). Breakables use it; nothing else pays for it.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Physics", default)]
#[serde(deny_unknown_fields)]
pub struct ContactReports {
    /// The weakest impact reported (N·s, at least 0).
    #[reflect(range(min = 0.0, max = 100000.0))]
    pub min_strength: f32,
}

impl ContactReports {
    /// Reports impacts at least `min_strength` hard; it must be finite and not negative.
    pub fn new(min_strength: f32) -> Result<Self, crate::PhysicsError> {
        match min_strength.is_finite() && min_strength >= 0.0 {
            true => Ok(Self { min_strength }),
            false => Err(crate::PhysicsError::Invalid(format!(
                "min_strength must be finite and at least 0, got {min_strength}"
            ))),
        }
    }
}
