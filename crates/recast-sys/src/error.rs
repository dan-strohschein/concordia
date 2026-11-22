//! Errors from the shim, with the Recast stage that failed.

/// The Recast build stage that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildStage {
    /// Creating the voxel heightfield.
    Heightfield,
    /// Rasterizing triangles.
    Rasterize,
    /// Compacting the heightfield.
    Compact,
    /// Eroding by the agent radius.
    Erode,
    /// Partitioning into regions.
    Regions,
    /// Tracing contours.
    Contours,
    /// Building polygons.
    PolyMesh,
    /// Building the detail mesh.
    Detail,
    /// Packing Detour tile data.
    NavData,
}

/// A Recast or Detour failure.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RecastError {
    /// Input was rejected before or by the shim.
    #[error("invalid navigation input: {0}")]
    Input(String),
    /// Recast or Detour ran out of memory.
    #[error("Recast/Detour ran out of memory")]
    Alloc,
    /// A build stage failed.
    #[error("Recast build failed at {0:?}")]
    Build(BuildStage),
    /// A tile could not be added (bad data, or its slot is taken).
    #[error("Detour refused the tile (bad data, or the tile is already present)")]
    AddTile,
    /// No polygon near the point, or a stale polygon reference.
    #[error("no navmesh polygon there")]
    NotFound,
    /// A Detour query failed.
    #[error("Detour query failed")]
    Query,
}

impl RecastError {
    /// The error for a shim status code (`CR_OK` and `CR_EMPTY` are not errors).
    pub(crate) fn from_code(code: i32) -> Self {
        match code {
            -2 => Self::Alloc,
            -10 => Self::Build(BuildStage::Heightfield),
            -11 => Self::Build(BuildStage::Rasterize),
            -12 => Self::Build(BuildStage::Compact),
            -13 => Self::Build(BuildStage::Erode),
            -14 => Self::Build(BuildStage::Regions),
            -15 => Self::Build(BuildStage::Contours),
            -16 => Self::Build(BuildStage::PolyMesh),
            -17 => Self::Build(BuildStage::Detail),
            -18 => Self::Build(BuildStage::NavData),
            -20 => Self::AddTile,
            -21 => Self::NotFound,
            -22 => Self::Query,
            _ => Self::Input(format!("the shim rejected the input (code {code})")),
        }
    }
}
