//! Navigation errors.

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;

/// A navigation failure.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NavError {
    /// A component's settings are invalid; the message names the field.
    #[error("{entity}: {message}")]
    Invalid {
        /// The entity whose component is invalid.
        entity: EntityId,
        /// What is wrong, naming the component and field.
        message: String,
    },
    /// The entity has no `NavMeshVolume` (or no `Transform`).
    #[error("{0} has no NavMeshVolume and Transform")]
    NoVolume(EntityId),
    /// More distinct area costs than Recast has area ids.
    #[error("{count} distinct NavArea costs; at most {max} are supported")]
    TooManyCosts {
        /// Distinct costs found.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// No navmesh polygon within reach of the point.
    #[error("no navmesh near {0}")]
    OffNavmesh(Vec3),
    /// Serialized navmesh data is malformed.
    #[error("navmesh data is malformed: {0}")]
    Data(String),
    /// The cache could not be read or written.
    #[error("navmesh cache: {0}")]
    Cache(String),
    /// Recast or Detour failed.
    #[error(transparent)]
    Recast(#[from] recast_sys::RecastError),
}
