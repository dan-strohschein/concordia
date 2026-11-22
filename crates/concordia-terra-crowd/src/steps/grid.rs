//! `crowd.grid`: the snapshot of last tick's state.

use std::collections::{BTreeSet, HashMap};

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_nav::{CrowdAgent, NavAgent};
use concordia_physics::{CharacterController, CharacterMotion};
use concordia_scene::builtin::Transform;
use concordia_terra::{SystemCtx, View};

use crate::grid::{CrowdGrid, GridEntry};
use crate::processors::standing;
use crate::state::CrowdState;

/// `crowd.grid`: the snapshot of last tick's state, and states for new crowd agents.
pub(crate) fn grid_step(view: &mut View<'_>, _: &SystemCtx<'_>) {
    let mut motions: HashMap<EntityId, Vec3> = HashMap::new();
    if let Ok(query) = view.query::<&CharacterMotion>() {
        query.for_each(|id, m| {
            motions.insert(id, m.velocity);
        });
    }
    let mut controllers: HashMap<EntityId, (f32, f32)> = HashMap::new();
    let mut characters = Vec::new();
    if let Ok(query) = view.query::<(&CharacterController, &Transform)>() {
        query.for_each(|id, (c, t)| {
            controllers.insert(id, (c.radius, c.height));
            characters.push((id, t.translation, c.radius, c.height));
        });
    }
    let mut entries = Vec::new();
    let mut crowd = BTreeSet::new();
    let mut cell = 0.0f32;
    if let Ok(query) = view.query::<(&NavAgent, &CrowdAgent, &Transform, &CrowdState)>() {
        query.for_each(|id, (agent, tuning, t, state)| {
            crowd.insert(id);
            cell = cell.max(tuning.collision_query_range);
            entries.push(GridEntry {
                id,
                position: t.translation,
                radius: agent.radius,
                height: agent.height,
                velocity: state.velocity,
                desired_velocity: state.desired_velocity,
                solid: controllers.contains_key(&id),
                crowd: true,
            });
        });
    }
    for (id, position, radius, height) in characters {
        if crowd.contains(&id) {
            continue;
        }
        entries.push(GridEntry {
            id,
            position,
            radius,
            height,
            velocity: motions.get(&id).copied().unwrap_or(Vec3::ZERO),
            desired_velocity: Vec3::ZERO,
            solid: true,
            crowd: false,
        });
    }
    give_states(view, &crowd);
    if let Ok(grid) = view.resource_mut::<CrowdGrid>() {
        *grid = CrowdGrid::new(entries, if cell > 0.0 { cell } else { 1.0 });
    }
}

/// Queues a standing state for each crowd agent without one (it joins from the next tick).
pub(crate) fn give_states(view: &mut View<'_>, stated: &BTreeSet<EntityId>) {
    let mut missing = Vec::new();
    if let Ok(query) = view.query::<(&NavAgent, &CrowdAgent, &Transform)>() {
        query.for_each(|id, (_, _, t)| {
            if !stated.contains(&id) {
                missing.push((id, t.translation));
            }
        });
    }
    if let Ok(commands) = view.commands() {
        for (id, at) in missing {
            commands.insert(id, standing(at));
        }
    }
}
