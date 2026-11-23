//! Driving one vehicle for one tick: what is ahead (vehicles, people, stop lines, ends), the IDM,
//! and moving along and between lanes. Reads only the graph, the snapshot and the grants.

use std::collections::BTreeSet;

use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};

use super::components::{Vehicle, VehicleState, VehicleStatus};
use super::graph::TrafficGraph;
use super::idm::{Ahead, Idm, acceleration, advance};
use super::route::wander;
use super::snapshot::{TrafficSnapshot, next_lane};

/// How far ahead a vehicle looks for what to follow, in meters.
pub(crate) const LOOK_AHEAD: f32 = 150.0;
/// A vehicle this close to its destination's end, and this slow, has arrived.
const ARRIVED_WITHIN: f32 = 0.5;
const ARRIVED_BELOW: f32 = 0.05;

/// What a vehicle sees this tick.
pub(crate) struct Road<'a> {
    pub graph: &'a TrafficGraph,
    pub snapshot: &'a TrafficSnapshot,
    /// (vehicle, connector) pairs let in this tick.
    pub grants: &'a BTreeSet<(EntityId, EntityId)>,
    pub tick: u64,
    pub dt: f32,
}

/// What driving a tick produced.
pub(crate) struct Driven {
    /// Its centre and the direction it faces.
    pub pose: Option<(Vec3, Vec3)>,
    /// It stopped at its destination's end this tick.
    pub arrived: Option<(EntityId, Vec3)>,
}

/// The request distance at a junction: twice the IDM's desired gap.
pub(crate) fn request_distance(desired_gap: f32) -> f32 {
    2.0 * desired_gap
}

/// Drives `id` one tick.
pub(crate) fn drive(
    road: &Road<'_>,
    id: EntityId,
    vehicle: &Vehicle,
    state: &mut VehicleState,
) -> Driven {
    let idle = Driven {
        pose: None,
        arrived: None,
    };
    if matches!(
        state.status,
        VehicleStatus::Idle | VehicleStatus::Failed | VehicleStatus::Arrived
    ) {
        return idle;
    }
    let Some(lane) = state.lane.and_then(|l| road.graph.lane(l)) else {
        state.status = VehicleStatus::Failed;
        return idle;
    };
    let (ahead, stop) = look_ahead(road, id, vehicle, state);
    let waiting =
        stop.is_some_and(|gap| gap <= request_distance(desired_gap(vehicle, state.speed)));
    match (waiting, state.status) {
        (true, VehicleStatus::Waiting) => {}
        (true, _) => {
            state.status = VehicleStatus::Waiting;
            state.waiting_since = road.tick;
        }
        (false, _) => state.status = VehicleStatus::Driving,
    }
    let idm = Idm {
        v0: vehicle.max_speed.min(lane.speed_limit),
        a: vehicle.max_acceleration,
        b: vehicle.comfortable_braking,
        t: vehicle.time_headway,
        s0: vehicle.min_gap,
    };
    let a = acceleration(&idm, state.speed, ahead);
    let (dx, v) = advance(state.speed, a, road.dt);
    state.speed = v;
    state.distance += dx;
    cross_lane_ends(road, id, state);
    let Some(lane) = state.lane.and_then(|l| road.graph.lane(l)) else {
        return idle;
    };
    let pose = lane.at(state.distance);
    let arrived = arrive(road, id, vehicle, state).map(|d| (d, pose.0));
    Driven {
        pose: Some(pose),
        arrived,
    }
}

/// The IDM's desired gap s₀ + vT + v² / (2√(ab)).
fn desired_gap(vehicle: &Vehicle, v: f32) -> f32 {
    let ab = (vehicle.max_acceleration * vehicle.comfortable_braking).sqrt();
    vehicle.min_gap + v * vehicle.time_headway + v * v / (2.0 * ab)
}

/// The nearest thing ahead within `LOOK_AHEAD` (its bumper gap and speed), and the distance from
/// the front to the next stop line it hasn't been let past.
fn look_ahead(
    road: &Road<'_>,
    id: EntityId,
    vehicle: &Vehicle,
    state: &VehicleState,
) -> (Option<Ahead>, Option<f32>) {
    let half = vehicle.length * 0.5;
    let mut best: Option<Ahead> = None;
    let mut consider = |gap: f32, speed: f32| {
        if gap <= LOOK_AHEAD && best.is_none_or(|b| gap < b.gap) {
            best = Some(Ahead { gap, speed });
        }
    };
    let Some(here) = state.lane else {
        return (None, None);
    };
    // On its own lane: the first vehicle ahead (by distance, then id), and the first person.
    let mine = (state.distance, id);
    if let Some(&(along, other)) = road
        .snapshot
        .on_lane(here)
        .iter()
        .find(|&&(along, o)| o != id && (along, o) > mine)
        && let Some(o) = road.snapshot.vehicle(other)
    {
        consider(along - o.length * 0.5 - state.distance - half, o.speed);
    }
    if let Some(&(along, r)) = road
        .snapshot
        .people(here)
        .iter()
        .find(|p| p.0 > state.distance)
    {
        consider(along - r - state.distance - half, 0.0);
    }
    // Along the lanes ahead.
    let mut stop = None;
    let mut cur = here;
    let mut offset = road.graph.lane(here).map_or(0.0, |l| l.length()) - state.distance;
    let mut route = state.route.iter();
    let mut next = next_lane(road.graph, id, state);
    if state.routed_for.is_some() {
        route.next();
    }
    loop {
        let Some(n) = next.filter(|n| road.graph.lane(*n).is_some()) else {
            consider(offset - half + vehicle.min_gap, 0.0);
            break;
        };
        let info = road.graph.lane(n).expect("checked");
        if info.junction.is_some() && !road.grants.contains(&(id, n)) {
            let gap = offset - half;
            if cur == here {
                stop = Some(gap);
            }
            consider(gap + vehicle.min_gap, 0.0);
            break;
        }
        if let Some(&(along, other)) = road.snapshot.on_lane(n).iter().find(|v| v.1 != id)
            && let Some(o) = road.snapshot.vehicle(other)
        {
            consider(offset + along - o.length * 0.5 - half, o.speed);
        }
        if let Some(&(along, r)) = road.snapshot.people(n).first() {
            consider(offset + along - r - half, 0.0);
        }
        offset += info.length();
        if offset > LOOK_AHEAD {
            break;
        }
        next = match state.routed_for {
            Some(_) => route.next().copied(),
            None => wander(id, n, &info.next),
        };
        cur = n;
    }
    (best, stop)
}

/// Moves past lane ends into the next lane while its centre is beyond the end.
fn cross_lane_ends(road: &Road<'_>, id: EntityId, state: &mut VehicleState) {
    for _ in 0..8 {
        let Some(len) = state
            .lane
            .and_then(|l| road.graph.lane(l))
            .map(|l| l.length())
        else {
            return;
        };
        if state.distance <= len {
            return;
        }
        let into = next_lane(road.graph, id, state).filter(|n| {
            road.graph
                .lane(*n)
                .is_some_and(|i| i.junction.is_none() || road.grants.contains(&(id, *n)))
        });
        let Some(n) = into else {
            state.distance = len;
            state.speed = 0.0;
            return;
        };
        state.distance -= len;
        state.lane = Some(n);
        if state.route.first() == Some(&n) {
            state.route.remove(0);
        }
    }
}

/// Arrival on its destination lane: stopped at its end, or stopped in the queue behind a vehicle
/// that has arrived there (within its minimum gap plus `ARRIVED_WITHIN`).
fn arrive(
    road: &Road<'_>,
    id: EntityId,
    vehicle: &Vehicle,
    state: &mut VehicleState,
) -> Option<EntityId> {
    let destination = state.routed_for?;
    if state.lane != Some(destination) || !state.route.is_empty() || state.speed >= ARRIVED_BELOW {
        return None;
    }
    let half = vehicle.length * 0.5;
    let left = road.graph.lane(destination)?.length() - state.distance - half;
    let mine = (state.distance, id);
    let queued = road
        .snapshot
        .on_lane(destination)
        .iter()
        .find(|&&(along, o)| o != id && (along, o) > mine)
        .and_then(|&(along, o)| Some((along, road.snapshot.vehicle(o)?)))
        .is_some_and(|(along, o)| {
            let gap = along - o.length * 0.5 - state.distance - half;
            o.status == VehicleStatus::Arrived && gap < vehicle.min_gap + ARRIVED_WITHIN
        });
    if left < ARRIVED_WITHIN || queued {
        state.speed = 0.0;
        state.status = VehicleStatus::Arrived;
        return Some(destination);
    }
    None
}

/// The rotation that turns +z to `dir`: a turn about y to its ground direction, then a pitch
/// about the vehicle's own x to its slope, so a vehicle never rolls.
pub(crate) fn heading(dir: Vec3) -> Quat {
    let flat = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
    if flat == Vec3::ZERO {
        return Quat::IDENTITY;
    }
    // Both lie in the ground plane, so the arc turns about ±y (glam turns a half turn about +y).
    let yaw = Quat::from_rotation_arc(Vec3::Z, flat);
    let local = yaw.inverse() * dir.normalize_or_zero();
    let slope = Vec3::new(0.0, local.y, local.z).normalize_or_zero();
    match slope == Vec3::ZERO {
        true => yaw,
        false => yaw * Quat::from_rotation_arc(Vec3::Z, slope),
    }
}
