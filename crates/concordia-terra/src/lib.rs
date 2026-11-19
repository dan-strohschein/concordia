//! Concordia ECS: archetype storage keyed by stable entity ids, typed and by-name
//! queries, sequential schedules, a deterministic fixed-step loop and world hashing.
//!
//! Storage is safe Rust: each column is a `Vec<T>` behind a small trait object.
//!
//! ```
//! use concordia_core::id::DeterministicIdGen;
//! use concordia_terra::World;
//! use concordia_reflect::Reflect;
//! use serde::{Deserialize, Serialize};
//!
//! /// Hit points.
//! #[derive(Clone, PartialEq, Serialize, Deserialize, Reflect)]
//! struct Health(f32);
//!
//! let mut world = World::new();
//! let id = world.spawn(&mut DeterministicIdGen::new(1))?;
//! world.insert(id, Health(10.0))?;
//! let mut query = world.query::<&mut Health>()?;
//! query.for_each(&mut world, |_, h| h.0 -= 1.0);
//! assert_eq!(world.get::<Health>(id).map(|h| h.0), Some(9.0));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod component;
mod dynamic;
mod entity;
mod error;
mod hash;
mod query;
mod resource;
mod schedule;
mod sim;
mod storage;
mod world;

pub use component::{Component, ReflectComponent, register_component};
pub use dynamic::{DynamicQuery, component_from_json};
pub use entity::Entity;
pub use error::{EcsError, QueryError};
pub use hash::{ParseHashError, WorldHash};
pub use query::{ElemAccess, QueryData, QueryElem, QueryFilter, QueryState, With, Without};
pub use resource::Resource;
pub use schedule::{FnSystem, Schedule, System, SystemCtx, system};
pub use sim::{SimClock, SimLoop};
pub use world::{Changes, EntityWorldMut, World};
