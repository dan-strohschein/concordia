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

mod command;
mod component;
mod dynamic;
mod entity;
mod error;
mod hash;
mod lod;
mod migrate;
mod named;
mod observer;
mod processor;
mod query;
mod resource;
mod schedule;
mod signal;
mod sim;
mod storage;
mod timer;
mod view;
mod world;

pub use command::{ApplyReport, CommandBuffer, Pending, Target, TerraStats};
pub use component::{Component, ReflectComponent, register_component, register_dynamic_component};
pub use dynamic::{DynamicQuery, JsonComponent, component_from_json};
pub use entity::Entity;
pub use error::{EcsError, QueryError, ScheduleError};
pub use hash::{ParseHashError, StateDigest, WorldHash};
pub use lod::{
    Due, LOD, LOD_LEVELS, LodPosition, LodProfile, LodStats, LodViewer, SimLod, is_due, lod_phase,
    lod_stats, lod_system, register_lod, splitmix64,
};
pub use named::{Batch, NamedQuery};
pub use observer::{Observe, observer};
pub use processor::{FnProcessor, Phase, Processor, ProcessorDecl, processor};
pub use query::{
    ElemAccess, QueryData, QueryElem, QueryFilter, QueryState, ReadSlice, With, Without,
};
pub use resource::Resource;
pub use schedule::{
    FnSystem, FnTickEnd, Observer, Schedule, System, SystemCtx, TickEnd, system, tick_end,
};
pub use sim::{SimClock, SimLoop, TimeScale};
pub use storage::kind::{CHUNK_ROWS, EntityStorage, StorageKind, StorageStats};
pub use timer::Timer;
pub use view::{View, ViewQuery};
pub use world::{Changes, EntityWorldMut, World};
