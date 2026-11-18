//! Shared scene test types and helpers.
#![allow(dead_code, missing_docs)]

use std::collections::BTreeMap;

use concordia_core::math::{Vec2, Vec3};
use concordia_reflect::{Reflect, TypeRegistry};
use concordia_terra::register_component;
use serde::{Deserialize, Serialize};

/// Movement mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Mode {
    /// Still.
    Idle,
    /// Walking speed.
    Walk(f32),
    /// Jump height and duration.
    Jump(f32, f32),
    /// Patrol route.
    Patrol {
        /// Waypoints.
        points: Vec<Vec3>,
        /// Loop.
        looping: bool,
    },
}

/// Nested data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Inner {
    /// A number.
    pub a: i64,
    /// A note.
    pub note: String,
}

/// A component exercising every shape the scene format supports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Gadget {
    /// Mode.
    pub mode: Mode,
    /// Optional label.
    pub label: Option<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Stats.
    pub stats: BTreeMap<String, f64>,
    /// Optional offset.
    pub offset: Option<Vec2>,
    /// Count.
    pub count: u32,
    /// Nested.
    pub inner: Inner,
    /// Pair.
    pub pair: (i32, bool),
}

/// Hit points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Health {
    /// Current.
    pub hp: f32,
}

/// Built-ins plus the test components.
pub fn registry() -> TypeRegistry {
    let mut reg = TypeRegistry::new();
    concordia_scene::builtin::register(&mut reg).unwrap();
    register_component::<Gadget>(&mut reg).unwrap();
    register_component::<Health>(&mut reg).unwrap();
    reg
}

/// Reads a fixture file.
pub fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every canonical fixture (all but `noncanonical.*`).
pub fn canonical_fixtures() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".scene.ron") && !n.starts_with("noncanonical"))
        .map(|n| (n.clone(), fixture(&n)))
        .collect();
    out.sort();
    out
}
