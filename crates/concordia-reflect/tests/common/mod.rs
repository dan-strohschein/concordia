//! Shared reflected test types (a "zoo" covering every supported shape).
#![allow(dead_code, missing_docs)]

pub mod arb;

use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec2, Vec3};
use concordia_reflect::{Reflect, TypeRegistry};
use serde::{Deserialize, Serialize};

/// A health pool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Health {
    /// Current hit points.
    #[reflect(default = 100.0, range(min = 0.0, max = 1000.0))]
    pub hp: f32,
    /// Maximum hit points.
    pub max: f32,
}

/// Local transform.
///
/// Relative to the parent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Transform {
    /// Position.
    pub translation: Vec3,
    /// Orientation.
    pub rotation: Quat,
    /// Per-axis scale.
    pub scale: Vec3,
}

/// Movement mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Mode {
    /// Standing still.
    Idle,
    /// Walking at a speed.
    Walk(f32),
    /// Jump height and duration.
    Jump(f32, f32),
    /// Patrol a route.
    Patrol {
        /// Waypoints.
        points: Vec<Vec3>,
        /// Loop back to the start.
        looping: bool,
    },
}

/// Display name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(transparent)]
pub struct Name(pub String);

/// A tag with no data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Marker;

/// A numbered label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Pair(pub i32, pub String);

/// Rejects unknown fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Strict {
    /// A byte.
    pub a: u8,
}

/// Every supported shape at once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Zoo {
    /// Name.
    pub name: Name,
    /// Health.
    pub health: Health,
    /// Mode.
    pub mode: Mode,
    /// Tags.
    pub tags: Vec<String>,
    /// Stats.
    pub stats: BTreeMap<String, f64>,
    /// Target entity.
    pub target: Option<EntityId>,
    /// Optional 2D offset.
    pub offset: Option<Vec2>,
    /// A pair.
    pub pair: Pair,
    /// A marker.
    pub marker: Marker,
    /// Fixed array.
    pub grid: [u8; 3],
    /// A tuple.
    pub tuple: (i32, bool),
    /// Cached; never serialized.
    #[serde(skip)]
    pub cache: u32,
}

/// A registry with the whole zoo.
pub fn zoo_registry() -> TypeRegistry {
    let mut reg = TypeRegistry::new();
    reg.register::<Zoo>().unwrap();
    reg.register::<Transform>().unwrap();
    reg.register::<Strict>().unwrap();
    reg
}

/// A finite sample `Zoo`.
pub fn zoo_sample() -> Zoo {
    Zoo {
        name: Name("z".into()),
        health: Health { hp: 1.0, max: 2.0 },
        mode: Mode::Idle,
        tags: vec![],
        stats: BTreeMap::new(),
        target: None,
        offset: None,
        pair: Pair(0, String::new()),
        marker: Marker,
        grid: [0; 3],
        tuple: (0, false),
        cache: 0,
    }
}
