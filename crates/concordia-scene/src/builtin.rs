//! Built-in components every Concordia project has, and the default headless schedule.

use std::num::NonZeroUsize;

use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};
use concordia_reflect::{Reflect, RegistryError, TypeRegistry};
use concordia_terra::{Phase, Schedule, SystemCtx, World, lod_system, register_component, system};
use serde::{Deserialize, Serialize};

/// Human-readable entity name. Not required to be unique.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Core", default = Name("Entity".into()))]
#[serde(transparent)]
pub struct Name(pub String);

/// Local transform relative to `Parent` (or to the world if there is no parent).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Core", default)]
pub struct Transform {
    /// Position in meters.
    pub translation: Vec3,
    /// Orientation (unit quaternion).
    pub rotation: Quat,
    /// Per-axis scale factor.
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

/// Hierarchy link to the parent entity's stable id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Core")]
#[serde(transparent)]
pub struct Parent(pub EntityId);

/// Velocity integrated into `Transform` by `integrate_velocity` every fixed tick.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Core", default)]
pub struct Velocity {
    /// Meters per second.
    pub linear: Vec3,
    /// Rotation axis scaled by radians per second.
    pub angular: Vec3,
}

/// Registers the built-in components.
pub fn register(registry: &mut TypeRegistry) -> Result<(), RegistryError> {
    register_component::<Name>(registry)?;
    register_component::<Transform>(registry)?;
    register_component::<Parent>(registry)?;
    register_component::<Velocity>(registry)
}

const CHUNK: NonZeroUsize = NonZeroUsize::new(1024).unwrap();

/// Moves and rotates every entity with `Transform` and `Velocity` by one fixed step.
pub fn integrate_velocity(world: &mut World, ctx: &SystemCtx<'_>) {
    let Ok(mut query) = world.query::<(&mut Transform, &Velocity)>() else {
        return;
    };
    let dt = ctx.dt;
    query.par_for_each(world, ctx.jobs, CHUNK, |_, (transform, velocity)| {
        transform.translation += velocity.linear * dt;
        if velocity.angular != Vec3::ZERO {
            transform.rotation =
                (Quat::from_scaled_axis(velocity.angular * dt) * transform.rotation).normalize();
        }
    });
}

/// The default headless schedule: `terra.lod` (first in `PrePhysics`, from world positions) and
/// `integrate_velocity`.
pub fn headless_schedule() -> Schedule {
    let mut schedule = Schedule::new();
    schedule.add_in(Phase::PrePhysics, lod_system(lod_position));
    schedule.add(system("integrate_velocity", integrate_velocity));
    schedule
}

/// World-space transforms of every entity with a `Transform`, from the `Parent` chain.
#[derive(Debug, Clone, Default)]
pub struct WorldTransforms {
    transforms: std::collections::BTreeMap<EntityId, concordia_core::math::Mat4>,
    /// Entities whose parent is missing or part of a cycle (they are treated as roots).
    pub problems: Vec<(EntityId, String)>,
}

impl WorldTransforms {
    /// The entity's world matrix.
    pub fn get(&self, id: EntityId) -> Option<&concordia_core::math::Mat4> {
        self.transforms.get(&id)
    }

    /// Every (entity, world matrix), sorted by id.
    pub fn iter(&self) -> impl Iterator<Item = (&EntityId, &concordia_core::math::Mat4)> {
        self.transforms.iter()
    }
}

/// Computes world transforms in `EntityId` order (deterministic); bad parents become roots.
pub fn world_transforms(world: &World) -> WorldTransforms {
    let mut out = WorldTransforms::default();
    for id in world.entity_ids_sorted() {
        let Some(t) = world.get::<Transform>(id) else {
            continue;
        };
        if world.get::<Parent>(id).is_none() {
            out.transforms.insert(id, local_matrix(t));
        } else if !out.transforms.contains_key(&id) {
            resolve(world, id, &mut out);
        }
    }
    out
}

/// A transform's matrix relative to its parent.
fn local_matrix(t: &Transform) -> concordia_core::math::Mat4 {
    concordia_core::math::Mat4::from_scale_rotation_translation(t.scale, t.rotation, t.translation)
}

/// One entity's world matrix (`None` without a `Transform`); same rules as [`world_transforms`].
pub fn world_transform(world: &World, id: EntityId) -> Option<concordia_core::math::Mat4> {
    world_matrix_with(id, &|e| world.get::<Transform>(e).copied(), &|e| {
        world.get::<Parent>(e).map(|p| p.0)
    })
}

/// One entity's world matrix from lookups (a processor's `Transform` and `Parent` columns):
/// the same rules as [`world_transform`]; `None` without a `Transform`.
pub fn world_matrix_with(
    id: EntityId,
    transform: &dyn Fn(EntityId) -> Option<Transform>,
    parent: &dyn Fn(EntityId) -> Option<EntityId>,
) -> Option<concordia_core::math::Mat4> {
    let mut chain = vec![(id, transform(id)?)];
    let mut seen = std::collections::BTreeSet::from([id]);
    while let Some(up) = chain.last().and_then(|(e, _)| parent(*e)) {
        let Some(t) = transform(up) else { break };
        if !seen.insert(up) {
            break;
        }
        chain.push((up, t));
    }
    Some(
        chain
            .iter()
            .rev()
            .fold(concordia_core::math::Mat4::IDENTITY, |base, (_, t)| {
                base * local_matrix(t)
            }),
    )
}

/// Walks up to the first resolved ancestor (or a root), then composes back down.
fn resolve(world: &World, id: EntityId, out: &mut WorldTransforms) {
    let mut chain = vec![id];
    let mut seen = std::collections::BTreeSet::from([id]);
    let mut base = concordia_core::math::Mat4::IDENTITY;
    while let Some(&Parent(parent)) = chain.last().and_then(|c| world.get::<Parent>(*c)) {
        let child = *chain.last().unwrap_or(&id);
        if let Some(resolved) = out.transforms.get(&parent) {
            base = *resolved;
            break;
        }
        if world.get::<Transform>(parent).is_none() {
            out.problems.push((
                child,
                format!("parent {parent} has no Transform or does not exist"),
            ));
            break;
        }
        if !seen.insert(parent) {
            out.problems
                .push((child, format!("parent {parent} is part of a cycle")));
            break;
        }
        chain.push(parent);
    }
    for entity in chain.into_iter().rev() {
        let t = world.get::<Transform>(entity).copied().unwrap_or_default();
        base *= local_matrix(&t);
        out.transforms.insert(entity, base);
    }
}

/// An entity's world position for simulation LOD (`terra.lod`); `None` without a `Transform`.
pub fn lod_position(world: &World, id: EntityId) -> Option<[f32; 3]> {
    let local = world.get::<Transform>(id)?;
    if world.get::<Parent>(id).is_none() {
        return Some(local.translation.to_array());
    }
    world_transform(world, id).map(|m| m.w_axis.truncate().to_array())
}
