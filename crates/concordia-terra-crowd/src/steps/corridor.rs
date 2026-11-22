//! `crowd.corridor`: bodies followed, corners and the visibility shortcut.

use concordia_core::math::Vec3;
use concordia_nav::{CrowdAgent, NavAgentState, NavMesh, NavMeshes};
use concordia_scene::builtin::Transform;
use concordia_terra::{SystemCtx, View};

use super::*;
use crate::corridor::{corners, merge_start_shortcut};
use crate::grid::CrowdGrid;
use crate::state::{CrowdState, MAX_CORNERS};
use crate::steer::flat;

/// `crowd.corridor`: a physics agent's corridor follows its body; corners along the corridor;
/// the visibility shortcut.
pub(crate) fn corridor_step(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let meshes = view.resource::<NavMeshes>().cloned().unwrap_or_default();
    let grid = view.resource::<CrowdGrid>().cloned().unwrap_or_default();
    let tick = ctx.tick.0;
    let Ok(query) = view.query::<(&CrowdAgent, &Transform, &mut NavAgentState, &mut CrowdState)>()
    else {
        return;
    };
    query.par_for_each(ctx.jobs, CHUNK, |id, (tuning, t, nav, state)| {
        let Some(mesh) = mesh_of(&meshes, nav) else {
            state.corners.clear();
            state.at_end = false;
            return;
        };
        if grid.get(id).is_some_and(|e| e.solid) {
            follow_body(mesh, nav, state, t.translation, tick);
        }
        pick_corners(mesh, nav, state);
        if tuning.optimize_visibility && walking(nav) {
            shortcut(mesh, nav, state, tuning.path_optimization_range);
        }
    });
}

/// `movePosition` for a physics agent: its corridor follows where physics put it.
pub(crate) fn follow_body(
    mesh: &NavMesh,
    nav: &mut NavAgentState,
    state: &mut CrowdState,
    at: Vec3,
    tick: u64,
) {
    if !move_on_surface(mesh, nav, state, at) {
        replan(nav, tick);
    }
}

/// `findCorners`: up to four straight-path corners from the agent along its corridor.
pub(crate) fn pick_corners(mesh: &NavMesh, nav: &NavAgentState, state: &mut CrowdState) {
    state.corners.clear();
    state.at_end = false;
    let Some(&end) = nav.waypoints.last().filter(|_| walking(nav)) else {
        return;
    };
    let Ok(points) = mesh.straight_path(state.surface, end, &nav.corridor, MAX_CORNERS + 1) else {
        return;
    };
    let mut found = corners(state.surface, &points);
    found.truncate(MAX_CORNERS);
    state.at_end = found
        .last()
        .is_some_and(|&c| flat(c - end).length() <= 1e-4);
    state.corners = found;
}

/// `optimizePathVisibility`: a ray toward the corner after next (stretched to `range`) that
/// reaches it shortcuts the corridor.
pub(crate) fn shortcut(mesh: &NavMesh, nav: &mut NavAgentState, state: &CrowdState, range: f32) {
    let Some(&next) = state
        .corners
        .get(state.corners.len().min(2).saturating_sub(1))
    else {
        return;
    };
    let from = state.surface;
    let dist = flat(next - from).length();
    if dist < 0.01 {
        return;
    }
    let stretched = (dist + 0.01).min(range);
    let goal = from + (next - from) * (range / stretched);
    let Some(&first) = nav.corridor.first() else {
        return;
    };
    if let Ok(ray) = mesh.raycast_path(first, from, goal, MAX_RAY) {
        let reached = ray.hit.is_none_or(|t| t > 0.99);
        if ray.polys.len() > 1 && reached {
            nav.corridor = merge_start_shortcut(&nav.corridor, &ray.polys);
        }
    }
}
