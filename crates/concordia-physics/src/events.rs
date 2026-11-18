//! Contact begin/end events, published once per tick.

use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Whether a contact began or ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Reflect)]
pub enum ContactKind {
    /// The bodies started touching (or overlapping, for sensors).
    Started,
    /// They stopped.
    Ended,
}

/// One contact change between two entities (`a < b`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Reflect)]
pub struct CollisionEvent {
    /// Lower entity id.
    pub a: EntityId,
    /// Higher entity id.
    pub b: EntityId,
    /// Started or ended.
    pub kind: ContactKind,
    /// One of them is a sensor.
    pub sensor: bool,
}

/// This tick's contact events, sorted; a play-world resource replaced every tick.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CollisionEvents(pub Vec<CollisionEvent>);
