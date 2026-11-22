//! Planning for `nav.plan`: which agents need a path, and one agent's plan.

use concordia_core::id::EntityId;
use concordia_core::math::Vec3;

use crate::agent::{NavAgent, NavAgentState, NavFailure, NavPathFailed, NavStatus};
use crate::crowd_agent::CrowdAgent;
use crate::module::NavMeshes;
use crate::navmesh::NavMesh;

/// A navmesh volume as agents see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Volume {
    pub id: EntityId,
    pub center: Vec3,
    pub half: Vec3,
    pub agent_radius: f32,
}

impl Volume {
    fn holds(&self, p: Vec3) -> bool {
        (p - self.center).abs().cmple(self.half).all()
    }
}

/// Updates an agent's status before the budget (idle without a target, waiting when it needs a
/// plan) and says whether it needs one.
pub(crate) fn wants_plan(
    agent: &NavAgent,
    state: &mut NavAgentState,
    position: Vec3,
    tick: u64,
    crowd: bool,
) -> bool {
    let Some(target) = agent.target else {
        if *state != NavAgentState::default() {
            *state = NavAgentState::default();
        }
        return false;
    };
    let same = state
        .planned_for
        .is_some_and(|p| agent.near(p, target, agent.target_desired_distance));
    let needs = match state.status {
        _ if !same => true,
        // A crowd agent's corridor follows it; the crowd asks for a replan when it loses it.
        NavStatus::Moving if crowd => false,
        NavStatus::Moving => off_path(state, position) > agent.path_max_distance,
        NavStatus::Idle | NavStatus::Waiting => true,
        NavStatus::Arrived | NavStatus::Failed => false,
    };
    if needs && state.status != NavStatus::Waiting {
        state.status = NavStatus::Waiting;
        state.waiting_since = tick;
    }
    needs
}

/// How far `position` is, sideways, from the segment the agent is walking.
fn off_path(state: &NavAgentState, position: Vec3) -> f32 {
    let next = state.next as usize;
    let (Some(a), Some(b)) = (
        next.checked_sub(1).and_then(|i| state.waypoints.get(i)),
        state.waypoints.get(next),
    ) else {
        return 0.0;
    };
    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
    let (a, b, p) = (flat(*a), flat(*b), flat(position));
    let ab = b - a;
    let t = match ab.length_squared() {
        l if l > 0.0 => ((p - a).dot(ab) / l).clamp(0.0, 1.0),
        _ => 0.0,
    };
    p.distance(a + ab * t)
}

/// What planning one agent gave.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Planned {
    /// A path (possibly partial) on `volume`'s navmesh.
    Path {
        volume: EntityId,
        waypoints: Vec<Vec3>,
        partial: bool,
        corridor: Vec<u64>,
    },
    /// No path.
    Failed { reason: NavFailure, detail: String },
}

/// What one agent's plan needs to know.
pub(crate) struct Request<'a> {
    pub entity: EntityId,
    pub agent: &'a NavAgent,
    pub target: Vec3,
    pub position: Vec3,
    pub controller: bool,
    /// The crowd's settings, for a crowd agent (which needs no controller).
    pub crowd: Option<&'a CrowdAgent>,
}

/// Plans `request` on the navmesh of its volume.
pub(crate) fn plan(request: &Request<'_>, volumes: &[Volume], meshes: &NavMeshes) -> Planned {
    let failed = |reason, detail: String| Planned::Failed { reason, detail };
    let Request {
        entity,
        agent,
        target,
        position,
        controller,
        crowd,
    } = *request;
    if let Err(e) = agent.validate(entity) {
        return failed(NavFailure::InvalidAgent, e.to_string());
    }
    if let Some(Err(e)) = crowd.map(|c| c.validate(entity)) {
        return failed(NavFailure::InvalidAgent, e.to_string());
    }
    if !controller && crowd.is_none() {
        let detail = format!("{entity} has a NavAgent but no CharacterController to move");
        return failed(NavFailure::InvalidAgent, detail);
    }
    let Some((volume, mesh)) = volume_for(agent, position, volumes, meshes) else {
        let detail = format!("no built navmesh volume holds {entity} at {position}");
        return failed(NavFailure::NoNavmesh, detail);
    };
    if mesh.nearest_point(position).is_none() {
        let detail = format!(
            "{entity} at {position} is too far from volume {}'s navmesh",
            volume.id
        );
        return failed(NavFailure::StartOffNavmesh, detail);
    }
    if mesh.nearest_point(target).is_none() {
        let detail = format!(
            "target {target} is too far from volume {}'s navmesh",
            volume.id
        );
        return failed(NavFailure::TargetOffNavmesh, detail);
    }
    match mesh.path(position, target) {
        Ok(path) => Planned::Path {
            volume: volume.id,
            waypoints: path.waypoints,
            partial: path.partial,
            corridor: match crowd {
                Some(_) => path.corridor,
                None => Vec::new(),
            },
        },
        Err(e) => failed(NavFailure::NoNavmesh, e.to_string()),
    }
}

/// The agent's volume (its own, or the first by id that holds it) and its navmesh.
pub(crate) fn volume_for<'a>(
    agent: &NavAgent,
    position: Vec3,
    volumes: &'a [Volume],
    meshes: &'a NavMeshes,
) -> Option<(&'a Volume, &'a NavMesh)> {
    let volume = match agent.volume {
        Some(id) => volumes.iter().find(|v| v.id == id),
        None => volumes.iter().find(|v| v.holds(position)),
    }?;
    Some((volume, meshes.get(volume.id)?.as_ref()))
}

/// Writes a plan into the agent's state, returning the failure signal to raise, if any.
pub(crate) fn apply(
    state: &mut NavAgentState,
    target: Vec3,
    planned: Planned,
) -> Option<NavPathFailed> {
    state.planned_for = Some(target);
    match planned {
        Planned::Path {
            volume,
            waypoints,
            partial,
            corridor,
        } => {
            state.status = NavStatus::Moving;
            state.next = u32::from(waypoints.len() > 1);
            state.waypoints = waypoints;
            state.partial = partial;
            state.volume = Some(volume);
            state.corridor = corridor;
            None
        }
        Planned::Failed { reason, detail } => {
            state.status = NavStatus::Failed;
            state.waypoints.clear();
            state.next = 0;
            state.partial = false;
            state.volume = None;
            state.corridor.clear();
            Some(NavPathFailed {
                target,
                reason,
                detail,
            })
        }
    }
}

/// Puts a crowd agent without a corridor on its navmesh: its polygon and volume (it can then be
/// pushed and kept on the navmesh before it walks). Off every navmesh, it stays unplaced.
pub(crate) fn place(
    agent: &NavAgent,
    state: &mut NavAgentState,
    position: Vec3,
    volumes: &[Volume],
    meshes: &NavMeshes,
) {
    if !state.corridor.is_empty() {
        return;
    }
    let Some((volume, mesh)) = volume_for(agent, position, volumes, meshes) else {
        return;
    };
    if let Some((poly, _)) = mesh.nearest_poly(position) {
        state.corridor = vec![poly];
        state.volume = Some(volume.id);
    }
}

/// Two sizes differ by more than a millimeter.
pub(crate) fn mismatch(a: f32, b: f32) -> bool {
    (a - b).abs() > 1e-3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::sideways;

    #[test]
    fn off_path_measures_sideways_from_the_current_segment() {
        let state = NavAgentState {
            waypoints: vec![
                Vec3::ZERO,
                Vec3::new(10.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 10.0),
            ],
            next: 1,
            ..NavAgentState::default()
        };
        assert_eq!(off_path(&state, Vec3::new(5.0, 3.0, 2.0)), 2.0);
        assert_eq!(
            off_path(&state, Vec3::new(-3.0, 0.0, 4.0)),
            5.0,
            "before the start"
        );
        assert_eq!(sideways(Vec3::ZERO, Vec3::new(3.0, 9.0, 4.0)), 5.0);
    }
}
