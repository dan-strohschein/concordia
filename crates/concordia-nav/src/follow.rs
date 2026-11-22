//! Following for `nav.follow`: past corners, arrival, and the velocity toward the next corner.

use concordia_core::math::Vec3;

use crate::agent::{NavAgent, NavAgentState, NavArrived, NavStatus, sideways};

/// One tick of following: the velocity to walk at, and the arrival to raise, if any.
pub(crate) fn follow(
    agent: &NavAgent,
    state: &mut NavAgentState,
    position: Vec3,
    dt: f32,
) -> (Vec3, Option<NavArrived>) {
    let walking = matches!(state.status, NavStatus::Moving | NavStatus::Waiting);
    let Some(last) = state.waypoints.len().checked_sub(1) else {
        return (Vec3::ZERO, None);
    };
    if !walking {
        return (Vec3::ZERO, None);
    }
    let mut next = (state.next as usize).min(last);
    while next < last && agent.near(position, state.waypoints[next], agent.path_desired_distance) {
        next += 1;
    }
    state.next = next as u32;
    let end = state.waypoints[last];
    if next == last && agent.near(position, end, agent.target_desired_distance) {
        return (Vec3::ZERO, arrive(agent, state, position));
    }
    (
        velocity(agent, position, state.waypoints[next], next == last, dt),
        None,
    )
}

/// Marks a moving agent arrived; a waiting one only stops at its old path's end.
fn arrive(agent: &NavAgent, state: &mut NavAgentState, position: Vec3) -> Option<NavArrived> {
    if state.status != NavStatus::Moving {
        return None;
    }
    state.status = NavStatus::Arrived;
    let target = agent.target.or(state.planned_for)?;
    Some(NavArrived {
        target,
        position,
        partial: state.partial,
    })
}

/// Toward `corner` at full speed, slowed on the last stretch so a tick never passes the end.
fn velocity(agent: &NavAgent, position: Vec3, corner: Vec3, last: bool, dt: f32) -> Vec3 {
    let gap = sideways(position, corner);
    if gap <= f32::EPSILON || dt.is_nan() || dt <= 0.0 {
        return Vec3::ZERO;
    }
    let speed = match last {
        true => agent.max_speed.min(gap / dt),
        false => agent.max_speed,
    };
    Vec3::new(corner.x - position.x, 0.0, corner.z - position.z) * (speed / gap)
}
