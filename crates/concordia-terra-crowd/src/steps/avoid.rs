//! `crowd.avoid`: the local boundary and sampled velocity obstacles.

use concordia_core::math::Vec3;
use concordia_nav::{CrowdAgent, CrowdAvoidance, NavAgent, NavAgentState, NavMeshes};
use concordia_terra::{SystemCtx, View};

use super::*;
use crate::avoid::{Circle, Me, Segment, sample_velocity, tri_area};
use crate::boundary::gather;
use crate::grid::{CrowdGrid, GridEntry};
use crate::state::{CrowdState, Wall};
use crate::steer::flat;

/// `crowd.avoid`: the velocity each walking agent picks among its neighbours and walls (the
/// desired one without avoidance).
pub(crate) fn avoid_step(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let meshes = view.resource::<NavMeshes>().cloned().unwrap_or_default();
    let grid = view.resource::<CrowdGrid>().cloned().unwrap_or_default();
    let Ok(query) = view.query::<(&NavAgent, &CrowdAgent, &NavAgentState, &mut CrowdState)>()
    else {
        return;
    };
    query.par_for_each(ctx.jobs, CHUNK, |id, (agent, tuning, nav, state)| {
        let params = match &tuning.avoidance {
            Some(p) if walking(nav) && state.desired_speed > 0.0 => p,
            _ => {
                state.new_velocity = state.desired_velocity;
                return;
            }
        };
        let position = grid.get(id).map_or(state.surface, |e| e.position);
        refresh_boundary(&meshes, nav, state, position, tuning.collision_query_range);
        state.new_velocity = choose(&grid, agent, params, state, position);
    });
}

/// `dtLocalBoundary::update` when there is no boundary yet or the agent has moved over a
/// quarter of `range` from where it was gathered.
fn refresh_boundary(
    meshes: &NavMeshes,
    nav: &NavAgentState,
    state: &mut CrowdState,
    at: Vec3,
    range: f32,
) {
    let threshold = range * 0.25;
    let stale = state
        .boundary_center
        .is_none_or(|c| flat(at - c).length_squared() > threshold * threshold);
    if !stale {
        return;
    }
    let walls = mesh_of(meshes, nav)
        .zip(nav.corridor.first())
        .and_then(|(mesh, &poly)| gather(mesh, poly, at, range));
    match walls {
        Some(walls) => {
            state.boundary_center = Some(at);
            state.walls = walls;
        }
        None => {
            state.boundary_center = None;
            state.walls.clear();
        }
    }
}

/// The sampled velocity among the neighbours (circles) and the walls in front (segments).
fn choose(
    grid: &CrowdGrid,
    agent: &NavAgent,
    params: &CrowdAvoidance,
    state: &CrowdState,
    position: Vec3,
) -> Vec3 {
    let circles: Vec<Circle> = state
        .neighbours
        .iter()
        .filter_map(|n| grid.get(*n))
        .map(circle)
        .collect();
    let segments = facing(&state.walls, position);
    let me = Me {
        position,
        radius: agent.radius,
        max_speed: state.desired_speed,
        velocity: state.velocity,
        desired: state.desired_velocity,
    };
    sample_velocity(me, &circles, &segments, params)
}

/// The walls `at` isn't behind (DetourCrowd skips `dtTriArea2D(pos, p, q) < 0`).
fn facing(walls: &[Wall], at: Vec3) -> Vec<Segment> {
    walls
        .iter()
        .filter(|w| tri_area(at, w.from, w.to) >= 0.0)
        .map(|w| Segment { p: w.from, q: w.to })
        .collect()
}

fn circle(e: &GridEntry) -> Circle {
    Circle {
        position: e.position,
        radius: e.radius,
        velocity: e.velocity,
        desired: e.desired_velocity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walls_behind_the_agent_are_skipped() {
        // From the origin, the wall (1, 0, 1) → (1, 0, −1) has Detour's area
        // acx·abz − abx·acz = 1 × 1 − 1 × (−1) = 2 ≥ 0: in front. Wound the other way it is −2:
        // the agent is behind it, so it is no obstacle.
        let front = Wall {
            from: Vec3::new(1.0, 0.0, 1.0),
            to: Vec3::new(1.0, 0.0, -1.0),
        };
        let behind = Wall {
            from: front.to,
            to: front.from,
        };
        let kept = facing(&[behind, front], Vec3::ZERO);
        assert_eq!(kept.len(), 1);
        assert_eq!((kept[0].p, kept[0].q), (front.from, front.to));
    }
}
