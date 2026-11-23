//! Concordia crowds (M2 phase 2.5c, ADR-0077): `NavAgent`s with a `CrowdAgent` steered together by
//! Terra processors in the parallel scheduler, following DetourCrowd's steps (zlib source) over
//! Detour queries: corridors, neighbours from a spatial grid, steering and separation,
//! integration, collision resolution and movement along the navmesh surface (N1, N5).

#![forbid(unsafe_code)]

mod avoid;
mod boundary;
mod collide;
mod corridor;
mod debug;
mod grid;
mod module;
mod motion;
mod processors;
mod state;
mod steer;
mod steps;
mod tables;
mod traits;

pub use grid::{CrowdGrid, GridEntry};
pub use module::{CROWD_LAYER, CrowdModule};
pub use processors::{
    AVOID, COLLIDE, CORRIDOR, GRID, INTEGRATE, MOVE, NEIGHBOURS, STEER, add_crowd_processors,
    insert_crowd_states,
};
pub use state::{CrowdState, Wall};
pub use traits::Crowd;

use concordia_reflect::{RegistryError, TypeRegistry};

/// Registers `CrowdState` and the `Crowd` trait.
pub fn register_types(registry: &mut TypeRegistry) -> Result<(), RegistryError> {
    concordia_terra::register_component::<CrowdState>(registry)?;
    concordia_terra_sim::register_trait::<Crowd>(registry)
}
