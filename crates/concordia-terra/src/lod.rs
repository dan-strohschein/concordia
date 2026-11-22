//! Simulation LOD (M2 phase 2.4b.1, ADR-0076): entities far from every `LodViewer` tick less
//! often, staggered by a fixed hash of their id, and LOD-aware processors get each entity's exact
//! elapsed scaled time. Concepts from Unreal's public Mass LOD documentation only (ADR-0067).

use std::num::NonZeroUsize;

use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::EcsError;
use crate::query::Without;
use crate::schedule::{System, SystemCtx};
use crate::sim::SimClock;
use crate::world::World;

/// Name of the built-in LOD system.
pub const LOD: &str = "terra.lod";

/// Number of simulation LOD levels (0 is full detail).
pub const LOD_LEVELS: usize = 4;

/// Marks an entity that simulation LOD is measured from (the player, a scene's camera rig).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "LOD", default)]
pub struct LodViewer;

/// Where simulation LOD levels start and how often each level ticks; shared per archetype.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "LOD", default)]
#[serde(deny_unknown_fields)]
pub struct LodProfile {
    /// Meters from the nearest viewer where levels 1, 2 and 3 start (ascending).
    pub distances: [f32; 3],
    /// Ticks between updates at levels 1, 2 and 3 (level 0 updates every tick).
    pub rates: [u32; 3],
    /// Meters an entity must come back inside a boundary before it regains detail.
    pub hysteresis: f32,
}

impl Default for LodProfile {
    fn default() -> Self {
        Self {
            distances: [25.0, 50.0, 100.0],
            rates: [2, 4, 8],
            hysteresis: 2.0,
        }
    }
}

impl LodProfile {
    /// A checked profile.
    pub fn new(distances: [f32; 3], rates: [u32; 3], hysteresis: f32) -> Result<Self, EcsError> {
        let profile = Self {
            distances,
            rates,
            hysteresis,
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Checks distances (finite, positive, ascending), rates (at least 1, ascending) and
    /// hysteresis (finite, at least 0).
    pub fn validate(&self) -> Result<(), EcsError> {
        let [d1, d2, d3] = self.distances;
        if !(d1.is_finite() && d3.is_finite() && d1 > 0.0 && d1 <= d2 && d2 <= d3) {
            return Err(invalid(
                "LOD profile distances",
                "finite, above 0 and ascending",
                format!("{:?}", self.distances),
            ));
        }
        let [r1, r2, r3] = self.rates;
        if !(r1 >= 1 && r1 <= r2 && r2 <= r3) {
            return Err(invalid(
                "LOD profile rates",
                "at least 1 and ascending",
                format!("{:?}", self.rates),
            ));
        }
        let h = self.hysteresis;
        if !(h.is_finite() && h >= 0.0 && h < d1) {
            return Err(invalid(
                "LOD profile hysteresis",
                "finite, at least 0 and below the first distance",
                h.to_string(),
            ));
        }
        Ok(())
    }

    /// Ticks between updates at `level` (1 for level 0).
    pub fn rate(&self, level: u8) -> u32 {
        match level {
            0 => 1,
            l => self.rates[usize::from(l.min(3)) - 1],
        }
    }

    /// The level for squared distance `dist_sq` from the nearest viewer, coming from `current`:
    /// detail is lost at a boundary and regained only inside it by the hysteresis margin.
    pub fn level(&self, current: u8, dist_sq: f32) -> u8 {
        let beyond = |margin: f32| {
            let past = self.distances.iter().filter(|&&d| {
                let edge = d - margin;
                edge * edge <= dist_sq
            });
            past.count() as u8
        };
        let raw = beyond(0.0);
        if raw >= current {
            raw
        } else {
            current.min(beyond(self.hysteresis))
        }
    }
}

/// An entity's simulation LOD this tick (hashed; written only by `terra.lod`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "LOD")]
#[serde(deny_unknown_fields)]
pub struct SimLod {
    /// 0 (full detail) to 3.
    pub level: u8,
    /// LOD-aware processors visit it this tick.
    pub due: bool,
    /// Scaled seconds this tick's update covers (0 when not due).
    pub elapsed: f64,
    /// Scaled time its simulation has reached.
    pub last_time: f64,
}

/// What a LOD-aware query hands each entity it visits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Due {
    /// Scaled seconds since the entity last ticked (the tick's `dt` at level 0).
    pub dt: f32,
    /// The same, at full precision.
    pub dt_exact: f64,
    /// Its level.
    pub level: u8,
}

/// The fixed stagger of `id`: due ticks are those where `(tick + phase) mod rate = 0`.
pub fn lod_phase(id: EntityId) -> u32 {
    let v = id.as_uuid().as_u128();
    let x = (v as u64) ^ ((v >> 64) as u64);
    (splitmix64(x) >> 32) as u32
}

/// One step of SplitMix64 (Steele, Lea and Flood, 2014): a fixed 64-bit mixer.
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Whether an entity with `phase` is due at `tick` at `rate`.
pub fn is_due(tick: u64, phase: u32, rate: u32) -> bool {
    let rate = u64::from(rate.max(1));
    (tick % rate + u64::from(phase) % rate).is_multiple_of(rate)
}

/// An entity's world position for LOD (`None` if it has none).
pub type LodPosition = fn(&World, EntityId) -> Option<[f32; 3]>;

/// The `terra.lod` exclusive system; add it first in `PrePhysics`.
pub fn lod_system(position: LodPosition) -> impl System {
    LodSystem { position }
}

struct LodSystem {
    position: LodPosition,
}

impl System for LodSystem {
    fn name(&self) -> &str {
        LOD
    }

    fn run(&mut self, world: &mut World, ctx: &SystemCtx<'_>) {
        let start = world.resource::<SimClock>().map_or(0.0, |c| c.time);
        let viewers = viewer_positions(world, self.position);
        sync_sim_lods(world, start);
        let mut work = profiled(world);
        let step = Step {
            tick: ctx.tick.0,
            end: start + ctx.dt_exact,
            viewers: &viewers,
            position: self.position,
        };
        let shared: &World = world;
        ctx.jobs.par_chunks_mut(&mut work, CHUNK, |_, items| {
            for (id, profile, lod) in items.iter_mut() {
                *lod = step.next(shared, *id, profile, *lod);
            }
        });
        write_sim_lods(world, &work);
    }
}

/// Entities per parallel piece in `terra.lod` (fixed, never from the thread count).
const CHUNK: NonZeroUsize = NonZeroUsize::new(1024).unwrap();

fn invalid(what: &'static str, expected: &'static str, value: String) -> EcsError {
    EcsError::InvalidValue {
        what,
        expected,
        value,
    }
}

/// Every viewer's position, in id order.
fn viewer_positions(world: &mut World, position: LodPosition) -> Vec<[f32; 3]> {
    let mut ids = Vec::new();
    if let Ok(mut query) = world.query::<&LodViewer>() {
        query.for_each(world, |id, _| ids.push(id));
    }
    ids.sort();
    ids.into_iter()
        .filter_map(|id| position(world, id))
        .collect()
}

/// Gives profiled entities a `SimLod` at level 0 (so their first tick is due: either the level
/// changes or level 0 ticks every tick) and takes it from the others.
fn sync_sim_lods(world: &mut World, start: f64) {
    let mut missing = Vec::new();
    if let Ok(mut query) = world.query_filtered::<&LodProfile, Without<SimLod>>() {
        query.for_each(world, |id, _| missing.push(id));
    }
    let mut stale = Vec::new();
    if let Ok(mut query) = world.query_filtered::<&SimLod, Without<LodProfile>>() {
        query.for_each(world, |id, _| stale.push(id));
    }
    let fresh = SimLod {
        level: 0,
        due: false,
        elapsed: 0.0,
        last_time: start,
    };
    for id in missing {
        let inserted = world.insert(id, fresh);
        debug_assert!(inserted.is_ok(), "the entity was just queried");
    }
    for id in stale {
        let removed = world.remove::<SimLod>(id);
        debug_assert!(removed.is_ok(), "the entity was just queried");
    }
}

/// Every profiled entity with its profile and current `SimLod`, in query order.
fn profiled(world: &mut World) -> Vec<(EntityId, LodProfile, SimLod)> {
    let mut work = Vec::new();
    if let Ok(mut query) = world.query::<(&LodProfile, &SimLod)>() {
        query.for_each(world, |id, (profile, lod)| work.push((id, *profile, *lod)));
    }
    work
}

/// Writes the new `SimLod`s back, in the order [`profiled`] read them.
fn write_sim_lods(world: &mut World, work: &[(EntityId, LodProfile, SimLod)]) {
    let Ok(mut query) = world.query::<&mut SimLod>() else {
        return;
    };
    let mut next = work.iter();
    query.for_each(world, |id, lod| {
        let found = next.as_slice().first().filter(|w| w.0 == id).is_some();
        if found && let Some((_, _, new)) = next.next() {
            *lod = *new;
        }
    });
    debug_assert!(next.as_slice().is_empty(), "every profiled entity written");
}

/// One tick's inputs to the level and due rules.
struct Step<'a> {
    tick: u64,
    end: f64,
    viewers: &'a [[f32; 3]],
    position: LodPosition,
}

impl Step<'_> {
    /// An entity's `SimLod` for this tick.
    fn next(&self, world: &World, id: EntityId, profile: &LodProfile, lod: SimLod) -> SimLod {
        let level = self.level(world, id, profile, lod.level);
        let changed = level != lod.level;
        let due = changed || is_due(self.tick, lod_phase(id), profile.rate(level));
        if due {
            SimLod {
                level,
                due,
                elapsed: self.end - lod.last_time,
                last_time: self.end,
            }
        } else {
            SimLod {
                level,
                due,
                elapsed: 0.0,
                last_time: lod.last_time,
            }
        }
    }

    /// Level 0 without viewers, a position or a valid profile.
    fn level(&self, world: &World, id: EntityId, profile: &LodProfile, current: u8) -> u8 {
        if self.viewers.is_empty() || profile.validate().is_err() {
            return 0;
        }
        let Some(p) = (self.position)(world, id) else {
            return 0;
        };
        let nearest = self
            .viewers
            .iter()
            .map(|v| {
                let d = [p[0] - v[0], p[1] - v[1], p[2] - v[2]];
                d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
            })
            .fold(f32::INFINITY, f32::min);
        profile.level(current, nearest)
    }
}

/// Counts for `terra.stats`: viewers, entities per level, and entities due this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LodStats {
    /// Entities with `LodViewer`.
    pub viewers: u64,
    /// Entities with a `SimLod`, per level.
    pub levels: [u64; LOD_LEVELS],
    /// Of those, due this tick.
    pub due: u64,
}

/// The world's LOD counts.
pub fn lod_stats(world: &World) -> LodStats {
    let mut stats = LodStats::default();
    for id in world.entity_ids_sorted() {
        if world.get::<LodViewer>(id).is_some() {
            stats.viewers += 1;
        }
        if let Some(lod) = world.get::<SimLod>(id) {
            stats.levels[usize::from(lod.level.min(3))] += 1;
            stats.due += u64::from(lod.due);
        }
    }
    stats
}

/// Registers `LodViewer`, `LodProfile` and `SimLod` with the type registry.
pub fn register_lod(
    registry: &mut concordia_reflect::TypeRegistry,
) -> Result<(), concordia_reflect::RegistryError> {
    crate::register_component::<LodViewer>(registry)?;
    crate::register_component::<LodProfile>(registry)?;
    crate::register_component::<SimLod>(registry)
}
