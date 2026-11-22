//! `nav.build` and `nav.path`.

use std::sync::Arc;

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, EntityRef, Method, WorldSel};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::module::NavMeshes;
use crate::navmesh::NavMesh;
use crate::service::{Builder, Built, NavState, api_error, no_volumes};

/// Build (or load from the cache) the navmesh of every `NavMeshVolume` in a world, or of one,
/// and report tiles, polygons and islands (connected parts: more than one often means a gap).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct NavBuild;

/// `nav.build` params.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct NavBuildParams {
    /// Only this volume (default: every one).
    pub volume: Option<EntityRef>,
    /// Which world (default edit; play uses the navmeshes built when play started).
    pub world: Option<WorldSel>,
}

/// One volume's navmesh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct NavBuildReport {
    /// The `NavMeshVolume` entity.
    pub volume: EntityId,
    /// Built tiles.
    pub tiles: u32,
    /// Polygons.
    pub polygons: u32,
    /// Connected parts.
    pub islands: u32,
    /// Lower corner of every polygon's bounds.
    pub min: Vec3,
    /// Upper corner.
    pub max: Vec3,
    /// Loaded unchanged rather than built.
    pub cached: bool,
    /// The cache key (hex).
    pub key: String,
}

/// `nav.build` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct NavBuildResult {
    /// One per volume, by entity id.
    pub navmeshes: Vec<NavBuildReport>,
}

impl Method for NavBuild {
    const NAME: &'static str = "nav.build";
    const MUTATES: bool = false;
    type Params = NavBuildParams;
    type Result = NavBuildResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: NavBuildParams,
    ) -> Result<NavBuildResult, ApiError> {
        let which = p.world.unwrap_or_default();
        let built = built(engine, which, p.volume.as_ref())?;
        if built.is_empty() {
            return Err(no_volumes(which));
        }
        Ok(NavBuildResult {
            navmeshes: built.iter().map(report).collect(),
        })
    }
}

/// Find a walking path between two points on a navmesh: corner waypoints from the start to the
/// end (both snapped to the navmesh), or to the closest reachable point when `partial`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct NavPathMethod;

/// `nav.path` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct NavPathParams {
    /// Start, world space (snapped to the navmesh within 2 m sideways).
    pub from: Vec3,
    /// Goal, world space (snapped the same way).
    pub to: Vec3,
    /// The volume whose navmesh to use (default: the first one under `from`).
    pub volume: Option<EntityRef>,
    /// Which world (default edit).
    pub world: Option<WorldSel>,
}

/// `nav.path` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct NavPathResult {
    /// Corner points, start and end included.
    pub waypoints: Vec<Vec3>,
    /// The goal is unreachable; the path ends at the closest reachable point.
    pub partial: bool,
    /// Length in meters.
    pub length: f32,
}

impl Method for NavPathMethod {
    const NAME: &'static str = "nav.path";
    const MUTATES: bool = false;
    type Params = NavPathParams;
    type Result = NavPathResult;

    fn call(engine: &mut Engine, _: &CallCtx, p: NavPathParams) -> Result<NavPathResult, ApiError> {
        let which = p.world.unwrap_or_default();
        let meshes = navmeshes(engine, which, p.volume.as_ref())?;
        if meshes.is_empty() {
            return Err(no_volumes(which));
        }
        let mesh = meshes
            .iter()
            .find(|m| m.nearest_point(p.from).is_some())
            .ok_or_else(|| api_error(crate::NavError::OffNavmesh(p.from)))?;
        let path = mesh.path(p.from, p.to).map_err(api_error)?;
        Ok(NavPathResult {
            waypoints: path.waypoints,
            partial: path.partial,
            length: path.length,
        })
    }
}

fn report(b: &Built) -> NavBuildReport {
    let stats = b.navmesh.stats();
    NavBuildReport {
        volume: b.volume,
        tiles: stats.tiles as u32,
        polygons: stats.polygons as u32,
        islands: stats.islands as u32,
        min: stats.min,
        max: stats.max,
        cached: b.cached,
        key: b.key.to_string(),
    }
}

/// The navmeshes of `which` (all volumes, or `only`), built or reused.
fn built(
    engine: &Engine,
    which: WorldSel,
    only: Option<&EntityRef>,
) -> Result<Vec<Built>, ApiError> {
    let world = engine.world(which)?;
    let state = engine.state::<NavState>().ok_or_else(|| {
        ApiError::new(ApiErrorKind::Internal, "the nav module's state is missing")
    })?;
    let builder = Builder {
        state,
        project: engine.project(),
        threads: engine.threads(),
    };
    match only {
        Some(volume) => Ok(vec![
            builder
                .build(world, which, volume.resolve(world)?)
                .map_err(api_error)?,
        ]),
        None => builder.build_all(world, which).map_err(api_error),
    }
}

/// The navmeshes queries use: the edit world's, current; the play world's, from play start.
pub(crate) fn navmeshes(
    engine: &Engine,
    which: WorldSel,
    only: Option<&EntityRef>,
) -> Result<Vec<Arc<NavMesh>>, ApiError> {
    if which == WorldSel::edit {
        return Ok(built(engine, which, only)?
            .into_iter()
            .map(|b| b.navmesh)
            .collect());
    }
    let world = engine.world(which)?;
    let all = world.resource::<NavMeshes>().cloned().unwrap_or_default();
    match only {
        Some(volume) => {
            let id = volume.resolve(world)?;
            let mesh = all
                .get(id)
                .cloned()
                .ok_or_else(|| api_error(crate::NavError::NoVolume(id)))?;
            Ok(vec![mesh])
        }
        None => Ok(all.iter().map(|(_, m)| m.clone()).collect()),
    }
}
