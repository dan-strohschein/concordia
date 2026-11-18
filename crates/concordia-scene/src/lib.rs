//! Concordia scenes and prefabs: the text source of truth.
//!
//! Canonical RON (ADR-0009): one `Entity(...)` block per entity sorted by id,
//! component keys and struct fields sorted, 4-space indent, trailing commas,
//! shortest round-trip floats. `format(format(x)) == format(x)`, and canonical
//! files survive load → save byte for byte.
//!
//! ```
//! use concordia_reflect::TypeRegistry;
//! use concordia_scene::{builtin, format};
//!
//! let mut registry = TypeRegistry::new();
//! builtin::register(&mut registry)?;
//! let text = r#"Scene(id: "0193a1c2-7e40-7000-8000-000000000001", format: 1, name: "S", entities: [])"#;
//! assert!(format(text, &registry)?.contains("    entities: [],\n"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod builtin;
mod convert;
mod error;
mod model;
mod prefab;
mod read;
mod syntax;
mod world;
mod write;

pub use error::SceneError;
pub use model::{EntityDoc, FORMAT_VERSION, PrefabDoc, SceneDoc, SceneMeta};
pub use prefab::instantiate_prefab;
pub use read::{read_prefab, read_scene};
pub use world::{load_into_world, snapshot_world};
pub use write::{format, write_prefab, write_scene};
