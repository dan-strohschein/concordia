//! The static, solid bodies the world collides with, by the same rules the physics step uses
//! (navigation builds its navmesh from them).

use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};
use concordia_terra::World;

use crate::components::{BodyKind, Collider};
use crate::runtime::{source_of, validate};

/// A static, non-sensor body physics accepts.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticSolid {
    /// Its entity.
    pub entity: EntityId,
    /// Its position (`Transform.translation`; scale never applies to colliders).
    pub translation: Vec3,
    /// Its orientation.
    pub rotation: Quat,
    /// Its collider (shape and offset).
    pub collider: Collider,
}

/// Every static solid in `world`, by entity id; bodies physics would reject are left out.
pub fn static_solids(world: &World) -> Vec<StaticSolid> {
    world
        .entity_ids_sorted()
        .into_iter()
        .filter_map(|entity| {
            let (t, source) = source_of(world, entity)?;
            let solid = source.body.kind == BodyKind::Static && !source.collider.is_sensor;
            (solid && validate(&source, &t).is_ok()).then_some(StaticSolid {
                entity,
                translation: t.translation,
                rotation: t.rotation,
                collider: source.collider,
            })
        })
        .collect()
}
