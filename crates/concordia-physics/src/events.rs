//! Contact begin/end signals (M2 phase 2.0b, ADR-0069): raised on both entities by the physics
//! step and delivered the next tick, in entity order.

use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Signal: this entity started touching (or, for sensors, overlapping) `other`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct ContactBegan {
    /// The other entity.
    pub other: EntityId,
    /// One of them is a sensor.
    pub sensor: bool,
}

/// Signal: this entity stopped touching (or overlapping) `other`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct ContactEnded {
    /// The other entity.
    pub other: EntityId,
    /// One of them is a sensor.
    pub sensor: bool,
}
