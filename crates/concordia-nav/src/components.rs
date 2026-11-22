//! Reflected navigation components (scene data): where navmeshes are built and how areas are marked.

use concordia_core::math::Vec3;
use concordia_reflect::{Reflect, RegistryError, TypeRegistry};
use concordia_terra::register_component;
use serde::{Deserialize, Serialize};

/// A navmesh is built inside this box (centred on the entity; rotation and scale are ignored)
/// from the static colliders it contains, for one agent size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Navigation", default)]
#[serde(default, deny_unknown_fields)]
pub struct NavMeshVolume {
    /// Half the box's size along each axis, in meters.
    pub half_extents: Vec3,
    /// Agent radius: walls and blocked areas are kept this far away.
    #[reflect(range(min = 0.0, max = 50.0))]
    pub agent_radius: f32,
    /// Agent height: the clearance a walkable surface needs.
    #[reflect(range(min = 0.1, max = 100.0))]
    pub agent_height: f32,
    /// Tallest step an agent climbs.
    #[reflect(range(min = 0.0, max = 10.0))]
    pub agent_max_climb: f32,
    /// Steepest walkable slope in degrees.
    #[reflect(range(min = 1.0, max = 89.0))]
    pub agent_max_slope_deg: f32,
    /// Voxel size on the ground plane (smaller is more precise and slower).
    #[reflect(range(min = 0.01, max = 10.0))]
    pub cell_size: f32,
    /// Voxel height.
    #[reflect(range(min = 0.01, max = 10.0))]
    pub cell_height: f32,
    /// Cells per tile side.
    #[reflect(range(min = 8.0, max = 1024.0))]
    pub tile_size: u32,
    /// How far outlines may cut inside walls, in cells (Recast's default 1.3; below 0.7 they are
    /// exact to the cell).
    #[reflect(range(min = 0.0, max = 3.0))]
    pub edge_max_error: f32,
}

impl Default for NavMeshVolume {
    /// A 100 × 20 × 100 m box for the default character controller's capsule.
    fn default() -> Self {
        Self {
            half_extents: Vec3::new(50.0, 10.0, 50.0),
            agent_radius: 0.3,
            agent_height: 1.8,
            agent_max_climb: 0.3,
            agent_max_slope_deg: 45.0,
            cell_size: 0.25,
            cell_height: 0.2,
            tile_size: 64,
            edge_max_error: 1.3,
        }
    }
}

/// What a [`NavArea`] does to the walkable ground inside it.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub enum NavAreaKind {
    /// Not walkable.
    #[default]
    Blocked,
    /// Walkable at `cost` times the normal travel cost.
    Cost {
        /// At least 1.
        cost: f32,
    },
}

/// Marks the walkable ground inside a box (centred on the entity, axis-aligned).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Navigation", default)]
#[serde(default, deny_unknown_fields)]
pub struct NavArea {
    /// Half the box's size along each axis, in meters.
    pub half_extents: Vec3,
    /// Blocked, or a travel cost.
    pub kind: NavAreaKind,
}

impl Default for NavArea {
    fn default() -> Self {
        Self {
            half_extents: Vec3::splat(1.0),
            kind: NavAreaKind::Blocked,
        }
    }
}

/// A walkability grid over the ground plane, applied after [`NavArea`]s. Cell (0, 0)'s corner is
/// the entity's position; rows run along +z, characters along +x: `.` keeps what the geometry
/// and areas say, `#` blocks, `o` makes walkable ground walkable again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Navigation", default)]
#[serde(default, deny_unknown_fields)]
pub struct NavAreaGrid {
    /// Cell size in meters.
    #[reflect(range(min = 0.01, max = 100.0))]
    pub cell_size: f32,
    /// One string per row, all the same length.
    pub rows: Vec<String>,
}

impl Default for NavAreaGrid {
    fn default() -> Self {
        Self {
            cell_size: 1.0,
            rows: Vec::new(),
        }
    }
}

/// Registers the navigation components.
pub fn register_types(registry: &mut TypeRegistry) -> Result<(), RegistryError> {
    register_component::<NavMeshVolume>(registry)?;
    register_component::<NavArea>(registry)?;
    register_component::<NavAreaGrid>(registry)?;
    register_component::<crate::agent::NavAgent>(registry)?;
    register_component::<crate::agent::NavAgentState>(registry)?;
    register_component::<crate::agent::NavArrived>(registry)?;
    register_component::<crate::agent::NavPathFailed>(registry)
}
