//! `crowd.integrate`: velocities and candidate positions.

use concordia_core::math::Vec3;
use concordia_nav::{CrowdAgent, NavAgentState};
use concordia_terra::{SystemCtx, View};

use super::*;
use crate::grid::CrowdGrid;
use crate::motion::integrate;
use crate::state::CrowdState;

/// `crowd.integrate`: accelerate toward the chosen velocity and take the step.
pub(crate) fn integrate_step(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let grid = view.resource::<CrowdGrid>().cloned().unwrap_or_default();
    let dt = ctx.dt;
    let Ok(query) = view.query::<(&CrowdAgent, &NavAgentState, &mut CrowdState)>() else {
        return;
    };
    query.par_for_each(ctx.jobs, CHUNK, |id, (tuning, nav, state)| {
        let position = grid.get(id).map_or(state.surface, |e| e.position);
        let end = match state.at_end && walking(nav) {
            true => state.corners.last().copied(),
            false => None,
        };
        let (velocity, candidate) = integrate(
            position,
            state.velocity,
            state.new_velocity,
            tuning.max_acceleration,
            dt,
            end,
        );
        state.velocity = velocity;
        state.candidate = candidate;
        state.correction = Vec3::ZERO;
    });
}
