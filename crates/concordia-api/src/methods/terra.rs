//! `terra.spawn_batch`, `terra.despawn_batch`, `terra.stats` and `terra.inspect` (M2 phase 2.0c):
//! batch work on the play world, and storage introspection for agents.

use std::collections::BTreeSet;

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_terra::{
    DynamicQuery, SimLoop, StorageKind, TerraStats as Counters, World, lod_stats,
};
use concordia_terra_sim::{Distribution, Spawner};
use serde::{Deserialize, Serialize};

use super::WorldSel;
use super::sim::ensure_play;
use crate::engine::{Engine, internal};
use crate::error::{ApiError, ApiErrorKind};
use crate::method::{CallCtx, Method, MethodInfo};
use crate::templates::{check_processors, compiled, template_error};

/// Most ids `terra.spawn_batch` returns.
const LISTED: usize = 1000;

/// Spawn many entities from a template into the play world (never journaled), placed with a seeded distribution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TerraSpawnBatch;

/// `terra.spawn_batch` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SpawnBatchParams {
    /// Template name.
    pub template: String,
    /// 1 to 1,000,000.
    pub count: u32,
    /// Default 0.
    pub seed: Option<u64>,
    /// Default: a 1 m grid.
    pub distribution: Option<Distribution>,
    /// Center (default origin).
    pub origin: Option<[f32; 3]>,
}

/// `terra.spawn_batch` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SpawnBatchResult {
    /// Spawned.
    pub count: u32,
    /// The first 1,000 ids.
    pub entities: Vec<EntityId>,
}

impl Method for TerraSpawnBatch {
    const NAME: &'static str = "terra.spawn_batch";
    const MUTATES: bool = false;
    type Params = SpawnBatchParams;
    type Result = SpawnBatchResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: SpawnBatchParams,
    ) -> Result<SpawnBatchResult, ApiError> {
        let spawner = Spawner {
            template: p.template.clone(),
            count: p.count,
            seed: p.seed.unwrap_or(0),
            distribution: p
                .distribution
                .unwrap_or(Distribution::Grid { spacing: 1.0 }),
        };
        let points = spawner
            .points()
            .map_err(|e| template_error(e).at("/count"))?;
        let template = compiled(engine, &p.template)?;
        let origin = Vec3::from(p.origin.unwrap_or_default());
        ensure_play(engine, None)?;
        let Engine { play, ids, .. } = engine;
        let play = play
            .as_mut()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NotPlaying))?;
        check_processors(&template, play.sim.schedule())?;
        let spawned = template
            .spawn_batch(&mut play.world, ids, p.count, |i| Some(origin + points[i]))
            .map_err(template_error)?;
        Ok(SpawnBatchResult {
            count: p.count,
            entities: spawned.into_iter().take(LISTED).collect(),
        })
    }
}

/// Despawn play-world entities by id, or every entity with the given components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TerraDespawnBatch;

/// `terra.despawn_batch` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct DespawnBatchParams {
    /// Entity ids.
    pub entities: Option<Vec<EntityId>>,
    /// Components every despawned entity has.
    pub with: Option<Vec<String>>,
}

/// `terra.despawn_batch` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct DespawnBatchResult {
    /// Despawned.
    pub count: u32,
}

impl Method for TerraDespawnBatch {
    const NAME: &'static str = "terra.despawn_batch";
    const MUTATES: bool = false;
    type Params = DespawnBatchParams;
    type Result = DespawnBatchResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: DespawnBatchParams,
    ) -> Result<DespawnBatchResult, ApiError> {
        if p.entities.is_none() && p.with.is_none() {
            return Err(ApiError::new(
                ApiErrorKind::InvalidParams,
                "give `entities`, `with`, or both",
            ));
        }
        let mut targets: BTreeSet<EntityId> = p.entities.unwrap_or_default().into_iter().collect();
        if let Some(with) = p.with {
            let query = DynamicQuery {
                with,
                without: Vec::new(),
            };
            let world = engine.world(WorldSel::play)?;
            let found = query.run(world, engine.registry()).map_err(|e| {
                ApiError::new(ApiErrorKind::InvalidParams, e.to_string()).at("/with")
            })?;
            targets.extend(found);
        }
        let world = engine.play_world_mut()?;
        let count = targets
            .into_iter()
            .filter(|id| world.despawn(*id).is_ok())
            .count();
        Ok(DespawnBatchResult {
            count: u32::try_from(count).map_err(internal)?,
        })
    }
}

/// Terra storage counts, skipped commands, and the play schedule's phases and waves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TerraStats;

/// `terra.stats` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct StatsParams {
    /// "edit" (default) or "play".
    pub world: Option<WorldSel>,
}

/// One phase of the play schedule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct PhaseStats {
    /// e.g. "PostPhysics".
    pub phase: String,
    /// In order; a wave's names run in parallel.
    pub waves: Vec<Vec<String>>,
}

/// Simulation LOD counts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct LodBuckets {
    /// Entities with `LodViewer`.
    pub viewers: u64,
    /// Entities with a `SimLod` at levels 0–3.
    pub levels: Vec<u64>,
    /// Of those, due this tick.
    pub due: u64,
}

/// `terra.stats` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct StatsResult {
    /// Entities.
    pub entities: u64,
    /// Archetypes.
    pub archetypes: u64,
    /// Columns.
    pub columns: u64,
    /// Shared and tag values.
    pub shared_values: u64,
    /// Commands whose target was gone.
    pub skipped_commands: u64,
    /// Play schedule (empty for edit).
    pub phases: Vec<PhaseStats>,
    /// Simulation LOD.
    pub lod: LodBuckets,
}

impl Method for TerraStats {
    const NAME: &'static str = "terra.stats";
    const MUTATES: bool = false;
    type Params = StatsParams;
    type Result = StatsResult;

    fn call(engine: &mut Engine, _: &CallCtx, p: StatsParams) -> Result<StatsResult, ApiError> {
        let which = p.world.unwrap_or_default();
        let world = engine.world(which)?;
        let storage = world.storage_stats();
        let sim = match which {
            WorldSel::play => engine.play.as_ref().map(|play| &play.sim),
            WorldSel::edit => None,
        };
        Ok(StatsResult {
            entities: world.len() as u64,
            archetypes: storage.archetypes as u64,
            columns: storage.columns as u64,
            shared_values: storage.shared_values as u64,
            skipped_commands: world
                .resource::<Counters>()
                .map_or(0, |c| c.skipped_commands),
            phases: sim.map(phases).unwrap_or_default(),
            lod: buckets(world),
        })
    }
}

/// How Terra stores one entity: archetype, chunk, component storage kinds, pending signals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TerraInspect;

/// `terra.inspect` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct InspectParams {
    /// The entity.
    pub entity: EntityId,
    /// "edit" (default) or "play".
    pub world: Option<WorldSel>,
}

/// One component's storage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ComponentStorage {
    /// Component.
    pub name: String,
    /// dense, shared or tag.
    pub storage: String,
}

/// One pending signal type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct PendingSignal {
    /// Signal type.
    pub name: String,
    /// Pending count.
    pub count: u32,
}

/// `terra.inspect` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct InspectResult {
    /// Archetype index.
    pub archetype: u64,
    /// Chunk index.
    pub chunk: u64,
    /// By name.
    pub components: Vec<ComponentStorage>,
    /// Delivered next tick.
    pub pending_signals: Vec<PendingSignal>,
}

impl Method for TerraInspect {
    const NAME: &'static str = "terra.inspect";
    const MUTATES: bool = false;
    type Params = InspectParams;
    type Result = InspectResult;

    fn call(engine: &mut Engine, _: &CallCtx, p: InspectParams) -> Result<InspectResult, ApiError> {
        let world: &World = engine.world(p.world.unwrap_or_default())?;
        let info = world.entity_storage(p.entity).ok_or_else(|| {
            ApiError::new(
                ApiErrorKind::EntityNotFound,
                format!("no entity {}", p.entity),
            )
            .at("/entity")
        })?;
        Ok(InspectResult {
            archetype: info.archetype as u64,
            chunk: info.chunk as u64,
            components: info
                .components
                .into_iter()
                .map(|(name, kind)| ComponentStorage {
                    name: name.to_owned(),
                    storage: storage_name(kind).to_owned(),
                })
                .collect(),
            pending_signals: info
                .pending_signals
                .into_iter()
                .map(|(name, count)| PendingSignal {
                    name: name.to_owned(),
                    count: u32::try_from(count).unwrap_or(u32::MAX),
                })
                .collect(),
        })
    }
}

/// The world's simulation LOD counts.
fn buckets(world: &World) -> LodBuckets {
    let stats = lod_stats(world);
    LodBuckets {
        viewers: stats.viewers,
        levels: stats.levels.to_vec(),
        due: stats.due,
    }
}

/// The play schedule's waves, by phase.
fn phases(sim: &SimLoop) -> Vec<PhaseStats> {
    sim.schedule()
        .waves()
        .into_iter()
        .map(|(phase, waves)| PhaseStats {
            phase: format!("{phase:?}"),
            waves: waves
                .into_iter()
                .map(|w| w.into_iter().map(str::to_owned).collect())
                .collect(),
        })
        .collect()
}

fn storage_name(kind: StorageKind) -> &'static str {
    match kind {
        StorageKind::Dense => "dense",
        StorageKind::Shared => "shared",
        StorageKind::Tag => "tag",
    }
}

pub(super) fn methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<TerraSpawnBatch>(),
        MethodInfo::of::<TerraDespawnBatch>(),
        MethodInfo::of::<TerraStats>(),
        MethodInfo::of::<TerraInspect>(),
    ]
}
