//! Navmeshes per world and volume, rebuilt only when their input changes (memo, then the DDC).

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::{Arc, Mutex};

use concordia_api::{ApiError, ApiErrorKind, WorldSel};
use concordia_core::ddc::{Ddc, DdcKey};
use concordia_core::id::EntityId;
use concordia_terra::World;

use crate::cache::NavCache;
use crate::components::NavMeshVolume;
use crate::error::NavError;
use crate::gather::NavInput;
use crate::navmesh::NavMesh;

/// A memo slot: (play world?, volume).
type Slot = (bool, EntityId);

/// The engine-lifetime memo of built navmeshes.
#[derive(Debug, Default)]
pub(crate) struct NavState {
    memo: Mutex<BTreeMap<Slot, (DdcKey, Arc<NavMesh>)>>,
}

/// A volume's navmesh and how it was obtained.
#[derive(Debug, Clone)]
pub(crate) struct Built {
    pub volume: EntityId,
    pub navmesh: Arc<NavMesh>,
    pub cached: bool,
    pub key: DdcKey,
}

/// Where and how navmeshes are built.
pub(crate) struct Builder<'a> {
    pub state: &'a NavState,
    pub project: &'a Path,
    pub threads: NonZeroUsize,
}

impl Builder<'_> {
    /// `volume`'s navmesh in `world`: the memo if its input is unchanged, else the DDC or a build.
    pub fn build(
        &self,
        world: &World,
        which: WorldSel,
        volume: EntityId,
    ) -> Result<Built, NavError> {
        let input = NavInput::gather(world, volume)?;
        let key = input.key();
        let slot = (which == WorldSel::play, volume);
        if let Some((k, navmesh)) = self
            .state
            .memo
            .lock()
            .ok()
            .and_then(|m| m.get(&slot).cloned())
            && k == key
        {
            return Ok(Built {
                volume,
                navmesh,
                cached: true,
                key,
            });
        }
        let cache = NavCache::new(Ddc::new(self.project.join(".concordia/ddc")));
        let loaded = cache.load_or_build(&input, self.threads)?;
        let navmesh = Arc::new(loaded.navmesh);
        if let Ok(mut memo) = self.state.memo.lock() {
            memo.insert(slot, (key, navmesh.clone()));
        }
        Ok(Built {
            volume,
            navmesh,
            cached: loaded.cached,
            key,
        })
    }

    /// Every volume's navmesh in `world`, by entity id.
    pub fn build_all(&self, world: &World, which: WorldSel) -> Result<Vec<Built>, NavError> {
        volumes(world)
            .into_iter()
            .map(|v| self.build(world, which, v))
            .collect()
    }
}

/// Entities with a `NavMeshVolume`, by id.
pub(crate) fn volumes(world: &World) -> Vec<EntityId> {
    world
        .entity_ids_sorted()
        .into_iter()
        .filter(|e| world.get::<NavMeshVolume>(*e).is_some())
        .collect()
}

/// The error for a world without volumes.
pub(crate) fn no_volumes(which: WorldSel) -> ApiError {
    ApiError::new(ApiErrorKind::EntityNotFound, format!("no NavMeshVolume in the {which:?} world"))
        .suggest(["spawn an entity with Transform and NavMeshVolume over static colliders (RigidBody Static + Collider)".to_owned()])
}

/// The Engine API error for a navigation failure.
pub(crate) fn api_error(e: NavError) -> ApiError {
    let kind = match &e {
        NavError::Invalid { .. } | NavError::TooManyCosts { .. } | NavError::OffNavmesh(_) => {
            ApiErrorKind::ValueInvalid
        }
        NavError::NoVolume(_) => ApiErrorKind::EntityNotFound,
        NavError::Data(_) | NavError::Cache(_) | NavError::Recast(_) => ApiErrorKind::Internal,
    };
    let error = ApiError::new(kind, e.to_string());
    match e {
        NavError::OffNavmesh(_) => error.suggest([
            "pick a point over walkable ground (nav.build reports each navmesh's bounds)"
                .to_owned(),
        ]),
        NavError::NoVolume(_) => {
            error.suggest(["pass an entity that has a NavMeshVolume".to_owned()])
        }
        _ => error,
    }
}
