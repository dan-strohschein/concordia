//! `crowd.steer`: desired velocities.

use concordia_core::math::Vec3;
use concordia_nav::{CrowdAgent, NavAgent, NavAgentState};
use concordia_terra::{SystemCtx, View};

use super::*;
use crate::grid::CrowdGrid;
use crate::state::CrowdState;
use crate::steer::{Near, Steering, desired, separate};

/// `crowd.steer`: the desired velocity (and, until avoidance, the chosen one).
pub(crate) fn steer_step(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let grid = view.resource::<CrowdGrid>().cloned().unwrap_or_default();
    let Ok(query) = view.query::<(&NavAgent, &CrowdAgent, &NavAgentState, &mut CrowdState)>()
    else {
        return;
    };
    query.par_for_each(ctx.jobs, CHUNK, |id, (agent, tuning, nav, state)| {
        let position = grid.get(id).map_or(state.surface, |e| e.position);
        let (velocity, speed) = match walking(nav) && !state.corners.is_empty() {
            true => {
                let steering = Steering {
                    position,
                    corners: &state.corners,
                    at_end: state.at_end,
                    radius: agent.radius,
                    max_speed: agent.max_speed,
                    anticipate_turns: tuning.anticipate_turns,
                };
                (desired(&steering), agent.max_speed)
            }
            false => (Vec3::ZERO, 0.0),
        };
        let velocity = match tuning.separation && speed > 0.0 {
            true => {
                let near: Vec<Near> = state
                    .neighbours
                    .iter()
                    .filter_map(|n| grid.get(*n))
                    .map(|e| Near {
                        position: e.position,
                    })
                    .collect();
                let range = tuning.collision_query_range;
                separate(
                    position,
                    velocity,
                    speed,
                    &near,
                    range,
                    tuning.separation_weight,
                )
            }
            false => velocity,
        };
        state.desired_velocity = velocity;
        state.desired_speed = speed;
        state.new_velocity = velocity;
    });
}
