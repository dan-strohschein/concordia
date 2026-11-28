//! Contact begin/end signals (M2 phase 2.0b, ADR-0069): raised on both entities by the physics
//! step and delivered the next tick, in entity order.

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Signal: this entity started touching (or, for sensors, overlapping) `other`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "Physics")]
#[serde(deny_unknown_fields)]
pub struct ContactBegan {
    /// The other entity.
    pub other: EntityId,
    /// One of them is a sensor.
    pub sensor: bool,
}

/// Signal: this entity stopped touching (or overlapping) `other`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "Physics")]
#[serde(deny_unknown_fields)]
pub struct ContactEnded {
    /// The other entity.
    pub other: EntityId,
    /// One of them is a sensor.
    pub sensor: bool,
}

/// Signal: a contact with `other` began this hard (M8 8.0b): the closing speed along the contact
/// normal times the pair's reduced mass, in N·s, at `point` (world space). Raised only on
/// entities with [`crate::ContactReports`] whose threshold it meets, and never for sensors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "Physics")]
#[serde(deny_unknown_fields)]
pub struct ContactImpact {
    /// The other entity.
    pub other: EntityId,
    /// How hard (N·s).
    pub strength: f32,
    /// Where the contact began.
    pub point: Vec3,
}
