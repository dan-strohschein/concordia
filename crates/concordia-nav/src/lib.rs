//! Concordia navigation (M2 phase 2.5a, ADR-0077): tiled navmeshes built with Recast from the
//! static colliders inside a [`NavMeshVolume`], with [`NavArea`] and [`NavAreaGrid`] marks;
//! Detour queries (nearest point, paths, surface raycasts); a DDC cache; the `nav.*` methods and
//! the `navmesh` debug layer.

#![forbid(unsafe_code)]

mod build;
mod cache;
mod components;
mod data;
mod debug;
mod error;
mod gather;
mod methods;
mod module;
mod navmesh;
mod service;
mod settings;
mod tessellate;

pub use build::build_tiles;
pub use cache::{Loaded, NavCache};
pub use components::{NavArea, NavAreaGrid, NavAreaKind, NavMeshVolume, register_types};
pub use data::NavMeshData;
pub use error::NavError;
pub use gather::NavInput;
pub use methods::{
    NavBuild, NavBuildParams, NavBuildReport, NavBuildResult, NavPathMethod, NavPathParams,
    NavPathResult,
};
pub use module::{NAVMESH_LAYER, NavMeshes, NavModule};
pub use navmesh::{NavMesh, NavPath, NavPoint, NavPoly, NavRay, NavStats};
pub use settings::NavSettings;
