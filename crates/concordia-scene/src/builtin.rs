//! Built-in components every Concordia project has, and the default headless schedule.

use std::num::NonZeroUsize;

use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};
use concordia_reflect::{Reflect, RegistryError, TypeRegistry};
use concordia_terra::{Schedule, SystemCtx, World, register_component, system};
use serde::{Deserialize, Serialize};

/// Human-readable entity name. Not required to be unique.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(transparent)]
pub struct Name(pub String);

/// Local transform relative to `Parent` (or to the world if there is no parent).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
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
#[serde(transparent)]
pub struct Parent(pub EntityId);

/// Velocity integrated into `Transform` by `integrate_velocity` every fixed tick.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
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

/// The default headless schedule: `integrate_velocity`.
pub fn headless_schedule() -> Schedule {
    let mut schedule = Schedule::new();
    schedule.add(system("integrate_velocity", integrate_velocity));
    schedule
}
