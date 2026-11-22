//! `crowd.move`: surface moves, physics velocities and arrival.

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_nav::{
    CrowdAgent, NavAgent, NavAgentState, NavArrived, NavMesh, NavMeshes, NavStatus,
};
use concordia_physics::{CharacterController, CharacterMotion};
use concordia_scene::builtin::Transform;
use concordia_terra::{CommandBuffer, SystemCtx, View, With, Without};

use super::*;
use crate::state::CrowdState;
use crate::steer::flat;

/// `crowd.move`: surface agents along the navmesh; physics agents through their motion; arrival.
pub(crate) fn move_step(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let meshes = view.resource::<NavMeshes>().cloned().unwrap_or_default();
    let (tick, dt) = (ctx.tick.0, ctx.dt);
    if let Ok(query) = view.query_filtered::<(
        &NavAgent,
        &mut Transform,
        &mut NavAgentState,
        &mut CrowdState,
    ), (With<CrowdAgent>, Without<CharacterController>)>()
    {
        query.par_for_each_deferred(ctx.jobs, CHUNK, |commands, id, (agent, t, nav, state)| {
            if let Some(mesh) = mesh_of(&meshes, nav) {
                slide(mesh, nav, state, tick);
                t.translation = state.surface;
            }
            arrive(commands, id, agent, nav, state, t.translation);
        });
    }
    if let Ok(query) = view.query_filtered::<(
        &NavAgent,
        &Transform,
        &mut CharacterMotion,
        &mut NavAgentState,
        &mut CrowdState,
    ), (With<CrowdAgent>, With<CharacterController>)>()
    {
        query.par_for_each_deferred(
            ctx.jobs,
            CHUNK,
            |commands, id, (agent, t, motion, nav, state)| {
                arrive(commands, id, agent, nav, state, t.translation);
                let push = match dt > 0.0 {
                    true => state.correction / dt,
                    false => Vec3::ZERO,
                };
                motion.velocity = flat(state.velocity + push);
            },
        );
    }
}

/// `movePosition` for a surface agent: along the navmesh from where it stands to its candidate.
pub(crate) fn slide(mesh: &NavMesh, nav: &mut NavAgentState, state: &mut CrowdState, tick: u64) {
    let to = state.candidate;
    if !move_on_surface(mesh, nav, state, to) {
        state.velocity = Vec3::ZERO;
        replan(nav, tick);
    }
}

/// Arrival (2.5b.1's rule): at the path's end within `target_desired_distance` sideways.
pub(crate) fn arrive(
    commands: &mut CommandBuffer,
    id: EntityId,
    agent: &NavAgent,
    nav: &mut NavAgentState,
    state: &mut CrowdState,
    at: Vec3,
) {
    let end = nav.waypoints.last().copied();
    let reached = end.is_some_and(|e| agent.near(at, e, agent.target_desired_distance));
    if nav.status != NavStatus::Moving || !state.at_end || !reached {
        return;
    }
    nav.status = NavStatus::Arrived;
    state.velocity = Vec3::ZERO;
    state.desired_velocity = Vec3::ZERO;
    state.new_velocity = Vec3::ZERO;
    state.correction = Vec3::ZERO;
    if let Some(target) = agent.target.or(nav.planned_for) {
        commands.signal(
            id,
            NavArrived {
                target,
                position: at,
                partial: nav.partial,
            },
        );
    }
}
