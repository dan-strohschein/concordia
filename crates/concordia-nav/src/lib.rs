//! Concordia navigation (M2 phase 2.5a, ADR-0077): tiled navmeshes built with Recast from the
//! static colliders inside a [`NavMeshVolume`], with [`NavArea`] and [`NavAreaGrid`] marks;
//! Detour queries (nearest point, paths, surface raycasts); a DDC cache; the `nav.*` methods and
//! the `navmesh` debug layer; agents that walk character controllers along paths (2.5b.1).

#![forbid(unsafe_code)]

mod agent;
mod agents;
mod build;
mod cache;
mod click;
mod components;
mod crowd_agent;
mod data;
mod debug;
mod error;
mod follow;
mod gather;
mod methods;
mod module;
mod navmesh;
mod plan;
mod service;
mod settings;
mod tessellate;

pub use agent::{
    NavAgent, NavAgentState, NavArrived, NavFailure, NavPathFailed, NavStatus, sideways,
};
pub use agents::{AgentConfig, FOLLOW, PLAN, add_agent_processors, insert_agent_states};
pub use build::build_tiles;
pub use cache::{Loaded, NavCache};
pub use click::{CLICK, ClickToMove, add_click_processor};
pub use components::{NavArea, NavAreaGrid, NavAreaKind, NavMeshVolume, register_types};
pub use crowd_agent::{CrowdAgent, CrowdAvoidance};
pub use data::NavMeshData;
pub use error::NavError;
pub use gather::NavInput;
pub use methods::{
    NavBuild, NavBuildParams, NavBuildReport, NavBuildResult, NavPathMethod, NavPathParams,
    NavPathResult,
};
pub use module::{NAV_PATHS_LAYER, NAVMESH_LAYER, NavMeshes, NavModule};
pub use navmesh::{NavMesh, NavPath, NavPoint, NavPoly, NavRay, NavStats, SurfaceMove, SurfaceRay};
pub use service::gathers;
pub use settings::NavSettings;
pub use tessellate::{SEGMENTS as CIRCLE_STEPS, unit as circle_step};
