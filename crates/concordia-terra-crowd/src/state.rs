//! A crowd agent's engine state (hashed; written only by the `crowd.*` processors).

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Most neighbours an agent considers (DetourCrowd's `DT_CROWDAGENT_MAX_NEIGHBOURS`).
pub(crate) const MAX_NEIGHBOURS: usize = 6;
/// Most corners an agent steers by (DetourCrowd's `DT_CROWDAGENT_MAX_CORNERS`).
pub(crate) const MAX_CORNERS: usize = 4;

/// What the crowd keeps for a crowd agent: where it stands on the navmesh, its velocities, the
/// corners it steers by and its neighbours.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Navigation", default)]
#[serde(default, deny_unknown_fields)]
pub struct CrowdState {
    /// Where it stands on the navmesh (its corridor's start).
    pub surface: Vec3,
    /// Its velocity.
    pub velocity: Vec3,
    /// The velocity steering wants.
    pub desired_velocity: Vec3,
    /// The speed steering wants.
    pub desired_speed: f32,
    /// The velocity chosen this tick (avoidance's, or the desired one).
    pub new_velocity: Vec3,
    /// Where it would be after integrating, before collisions (within a tick).
    pub candidate: Vec3,
    /// A physics agent's collision push this tick (handed to its body as velocity).
    pub correction: Vec3,
    /// The next corners along its corridor (at most 4).
    pub corners: Vec<Vec3>,
    /// The last corner is the path's end.
    pub at_end: bool,
    /// Neighbours by distance, nearest first (at most 6).
    pub neighbours: Vec<EntityId>,
}
