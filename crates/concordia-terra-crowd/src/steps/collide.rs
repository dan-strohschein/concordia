//! `crowd.collide`: Jacobi collision iterations.

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_nav::NavAgent;
use concordia_terra::{SystemCtx, View};

use super::*;
use crate::collide::{Contact, ITERATIONS, displacement};
use crate::grid::CrowdGrid;
use crate::state::CrowdState;

/// `crowd.collide`: Jacobi iterations, each reading only the previous iteration's candidates.
pub(crate) fn collide_step(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let grid = view.resource::<CrowdGrid>().cloned().unwrap_or_default();
    for _ in 0..ITERATIONS {
        let mut snapshot: Vec<Option<Vec3>> = vec![None; grid.entries().len()];
        if let Ok(query) = view.query::<&CrowdState>() {
            query.for_each(|id, s| {
                if let Some(slot) = grid.slot(id) {
                    snapshot[slot] = Some(s.candidate);
                }
            });
        }
        let Ok(query) = view.query::<(&NavAgent, &mut CrowdState)>() else {
            return;
        };
        query.par_for_each(ctx.jobs, CHUNK, |id, (agent, state)| {
            let physics = grid.get(id).is_some_and(|e| e.solid);
            let contacts = contacts(&grid, &snapshot, &state.neighbours, physics);
            let push = displacement(
                id,
                state.candidate,
                agent.radius,
                state.desired_velocity,
                &contacts,
            );
            state.candidate += push;
            if physics {
                state.correction += push;
            }
        });
    }
}

/// Who an agent collides with: a surface agent with everyone near (taking the whole push from a
/// solid), a physics agent only with other physics crowd agents (Jolt doesn't keep them apart).
pub(crate) fn contacts(
    grid: &CrowdGrid,
    snapshot: &[Option<Vec3>],
    neighbours: &[EntityId],
    physics: bool,
) -> Vec<Contact> {
    neighbours
        .iter()
        .filter_map(|n| grid.slot(*n))
        .map(|slot| (slot, &grid.entries()[slot]))
        .filter(|(_, e)| !physics || (e.crowd && e.solid))
        .map(|(slot, e)| Contact {
            id: e.id,
            position: snapshot[slot].unwrap_or(e.position),
            radius: e.radius,
            whole: !physics && e.solid,
        })
        .collect()
}
