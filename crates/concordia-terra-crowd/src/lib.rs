//! Concordia crowds (M2 phase 2.5c, ADR-0077): `NavAgent`s with a `CrowdAgent` steered together by
//! Terra processors in the parallel scheduler, following DetourCrowd's steps (zlib source) over
//! Detour queries: corridors, neighbours from a spatial grid, steering and separation,
//! integration, collision resolution and movement along the navmesh surface (N1, N5); and lane
//! traffic (2.5d, ADR-0079): vehicles on the Intelligent Driver Model along lanes and junctions.

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
mod traffic;
mod traits;

pub use grid::{CrowdGrid, GridEntry};
pub use module::{CROWD_LAYER, CrowdModule, LANES_LAYER};
pub use processors::{
    AVOID, COLLIDE, CORRIDOR, GRID, INTEGRATE, MOVE, NEIGHBOURS, STEER, add_crowd_processors,
    insert_crowd_states,
};
pub use state::{CrowdState, Wall};
pub use traffic::components::{
    Grant, Junction, JunctionState, Lane, TrafficProblems, Vehicle, VehicleArrived,
    VehicleRouteFailed, VehicleState, VehicleStatus,
};
pub use traffic::graph::{JunctionInfo, LaneInfo, Projection, TrafficGraph, TrafficProblem};
pub use traffic::snapshot::{TrafficSnapshot, VehicleSnap};
pub use traffic::steps::{
    DRIVE, JUNCTIONS, PLACE, SNAPSHOT, add_traffic_processors, insert_traffic,
};
pub use traffic::traits::Traffic;
pub use traits::Crowd;

use concordia_reflect::{RegistryError, TypeRegistry};

/// Registers `CrowdState`, traffic's components and signals, and the `Crowd` and `Traffic` traits.
pub fn register_types(registry: &mut TypeRegistry) -> Result<(), RegistryError> {
    use concordia_terra::register_component;
    register_component::<CrowdState>(registry)?;
    register_component::<Lane>(registry)?;
    register_component::<Junction>(registry)?;
    register_component::<Vehicle>(registry)?;
    register_component::<VehicleState>(registry)?;
    register_component::<JunctionState>(registry)?;
    register_component::<TrafficProblems>(registry)?;
    register_component::<VehicleArrived>(registry)?;
    register_component::<VehicleRouteFailed>(registry)?;
    concordia_terra_sim::register_trait::<Crowd>(registry)?;
    concordia_terra_sim::register_trait::<Traffic>(registry)
}
