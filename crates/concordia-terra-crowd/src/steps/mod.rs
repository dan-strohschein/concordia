//! The crowd's steps, one module each, and what they share.

pub(crate) mod avoid;
pub(crate) mod collide;
pub(crate) mod corridor;
pub(crate) mod grid;
pub(crate) mod integrate;
pub(crate) mod movement;
pub(crate) mod neighbours;
pub(crate) mod steer;

use std::num::NonZeroUsize;

use concordia_core::math::Vec3;
use concordia_nav::{NavAgentState, NavMesh, NavMeshes, NavStatus};

use crate::corridor::merge_start_moved;
use crate::state::CrowdState;

/// Agents per parallel piece (fixed, never from the thread count).
pub(crate) const CHUNK: NonZeroUsize = NonZeroUsize::new(256).unwrap();
/// Polygons a surface move may cross (DetourCrowd's `MAX_VISITED`).
pub(crate) const MAX_VISITED: usize = 16;
/// Polygons a visibility shortcut's ray may cross (DetourCrowd's `MAX_RES`).
pub(crate) const MAX_RAY: usize = 32;

/// The navmesh an agent's corridor is on.
pub(crate) fn mesh_of<'m>(meshes: &'m NavMeshes, nav: &NavAgentState) -> Option<&'m NavMesh> {
    meshes.get(nav.volume?).map(|m| m.as_ref())
}

/// Walking: following a planned path (or the old one while a new plan waits).
pub(crate) fn walking(nav: &NavAgentState) -> bool {
    matches!(nav.status, NavStatus::Moving | NavStatus::Waiting)
        && !nav.waypoints.is_empty()
        && !nav.corridor.is_empty()
}

/// Asks `nav.plan` for a new path (the corridor was lost).
pub(crate) fn replan(nav: &mut NavAgentState, tick: u64) {
    if nav.status != NavStatus::Waiting && nav.planned_for.is_some() {
        nav.status = NavStatus::Waiting;
        nav.waiting_since = tick;
    }
}

/// `movePosition`: moves along the navmesh from where the agent stands toward `to`, merges the
/// polygons crossed into its corridor and puts it on the surface. `false` if the move failed.
pub(crate) fn move_on_surface(
    mesh: &NavMesh,
    nav: &mut NavAgentState,
    state: &mut CrowdState,
    to: Vec3,
) -> bool {
    let Some(&first) = nav.corridor.first() else {
        return true;
    };
    let Ok(moved) = mesh.move_along_surface(first, state.surface, to, MAX_VISITED) else {
        return false;
    };
    nav.corridor = merge_start_moved(&nav.corridor, &moved.visited);
    let y = nav
        .corridor
        .first()
        .and_then(|&p| mesh.poly_height(p, moved.position).ok().flatten());
    state.surface = Vec3::new(
        moved.position.x,
        y.unwrap_or(state.surface.y),
        moved.position.z,
    );
    true
}
