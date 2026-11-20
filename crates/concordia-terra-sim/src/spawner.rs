//! Spawners (M2 phase 2.0c): an edit-world component that places a template's entities with a
//! seeded distribution when play starts.

use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::math::Vec3;
use concordia_core::rng::SimRng;
use concordia_reflect::{Reflect, TypeRegistry};
use concordia_scene::builtin::world_transform;
use concordia_terra::World;
use serde::{Deserialize, Serialize};

use crate::catalog::TemplateCatalog;
use crate::error::TemplateError;

/// How a spawner arranges its entities around its transform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Distribution {
    /// A square grid on the XZ plane, `spacing` meters apart, centered.
    Grid {
        /// Meters between neighbors.
        spacing: f32,
    },
    /// Uniformly in a box of these half extents.
    Box {
        /// Half size on each axis, in meters.
        half_extents: Vec3,
    },
    /// Uniformly in a disc of this radius on the XZ plane.
    Disc {
        /// Radius in meters.
        radius: f32,
    },
}

/// Places `count` entities of `template` when play starts (the edit world keeps only this).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
#[reflect(category = "Simulation")]
pub struct Spawner {
    /// Template name (see entity.templates).
    pub template: String,
    /// How many entities (1 to 1,000,000).
    pub count: u32,
    /// Seed for the distribution and the spawned ids.
    pub seed: u64,
    /// Where they go, relative to the spawner's transform.
    pub distribution: Distribution,
}

impl Spawner {
    /// Most entities one spawner may place.
    pub const MAX_COUNT: u32 = 1_000_000;

    /// The offsets of its entities, in order (deterministic from the seed; no trigonometry, so
    /// no platform math differences).
    pub fn points(&self) -> Result<Vec<Vec3>, TemplateError> {
        if !(1..=Self::MAX_COUNT).contains(&self.count) {
            return Err(invalid(
                "count",
                format!("must be 1 to {}, got {}", Self::MAX_COUNT, self.count),
            ));
        }
        let n = self.count as usize;
        let mut rng = SimRng::new(self.seed);
        match &self.distribution {
            Distribution::Grid { spacing } => Ok(grid(n, positive("spacing", *spacing)?)),
            Distribution::Box { half_extents } => {
                let h = extents(*half_extents)?;
                Ok((0..n)
                    .map(|_| {
                        Vec3::new(
                            signed(&mut rng) * h.x,
                            signed(&mut rng) * h.y,
                            signed(&mut rng) * h.z,
                        )
                    })
                    .collect())
            }
            Distribution::Disc { radius } => {
                let r = positive("radius", *radius)?;
                Ok((0..n).map(|_| disc(&mut rng) * r).collect())
            }
        }
    }
}

fn invalid(what: &str, reason: String) -> TemplateError {
    TemplateError::InvalidValue {
        what: format!("Spawner {what}"),
        reason,
    }
}

fn positive(what: &str, x: f32) -> Result<f32, TemplateError> {
    if x.is_finite() && x > 0.0 {
        Ok(x)
    } else {
        Err(invalid(
            what,
            format!("must be a finite number above 0, got {x}"),
        ))
    }
}

fn extents(h: Vec3) -> Result<Vec3, TemplateError> {
    if h.is_finite() && h.min_element() >= 0.0 {
        Ok(h)
    } else {
        Err(invalid(
            "half_extents",
            format!("must be finite and at least 0, got {h}"),
        ))
    }
}

/// A centered square grid on XZ, row by row.
fn grid(n: usize, spacing: f32) -> Vec<Vec3> {
    let side = (n as f64).sqrt().ceil() as usize;
    let center = (side as f32 - 1.0) / 2.0;
    (0..n)
        .map(|k| {
            let (row, col) = (k / side, k % side);
            Vec3::new(
                (col as f32 - center) * spacing,
                0.0,
                (row as f32 - center) * spacing,
            )
        })
        .collect()
}

/// Uniform in [-1, 1).
fn signed(rng: &mut SimRng) -> f32 {
    rng.next_f32() * 2.0 - 1.0
}

/// Uniform in the unit disc on XZ, by rejection.
fn disc(rng: &mut SimRng) -> Vec3 {
    loop {
        let (x, z) = (signed(rng), signed(rng));
        if x * x + z * z <= 1.0 {
            return Vec3::new(x, 0.0, z);
        }
    }
}

/// Expands every spawner in `world` (in id order) at its world transform; returns the spawned
/// ids in order. Ids come from the spawner's seed and id, so every play session matches.
pub fn expand_spawners(
    world: &mut World,
    catalog: &TemplateCatalog,
    registry: &TypeRegistry,
) -> Result<Vec<EntityId>, TemplateError> {
    let spawners: Vec<(EntityId, Spawner)> = world
        .entity_ids_sorted()
        .into_iter()
        .filter_map(|id| Some((id, world.get::<Spawner>(id)?.clone())))
        .collect();
    let mut spawned = Vec::new();
    for (id, spawner) in spawners {
        let compiled = catalog.get(&spawner.template)?.compile(registry)?;
        let points = spawner.points()?;
        let at = world_transform(world, id).unwrap_or_default();
        let bits = id.as_uuid().as_u128();
        let mut ids = DeterministicIdGen::new(spawner.seed ^ (bits as u64) ^ ((bits >> 64) as u64));
        spawned.extend(compiled.spawn_batch(world, &mut ids, spawner.count, |i| {
            Some(at.transform_point3(points[i]))
        })?);
    }
    Ok(spawned)
}
