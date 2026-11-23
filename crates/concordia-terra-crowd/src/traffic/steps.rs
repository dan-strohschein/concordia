//! Traffic's processors in `PrePhysics`, after the crowd: place, snapshot, junctions, drive. Each
//! writes only its own entity and reads others from the snapshot, so the hash is the same at any
//! thread count.

use std::collections::BTreeSet;

use concordia_core::id::EntityId;
use concordia_scene::builtin::{Transform, world_transform};
use concordia_terra::{Phase, Schedule, SystemCtx, View, Without, World, processor};

use super::components::{
    Grant, Junction, JunctionState, Lane, TrafficProblems, Vehicle, VehicleArrived,
    VehicleRouteFailed, VehicleState, VehicleStatus,
};
use super::drive::{Road, drive, heading, request_distance};
use super::graph::{LaneInput, TrafficGraph};
use super::junction::{Request, grant};
use super::route::route;
use super::snapshot::TrafficSnapshot;
use crate::grid::CrowdGrid;
use crate::processors::MOVE;
use crate::steps::CHUNK;

/// `traffic.place`: puts new vehicles on their nearest lane and routes them.
pub const PLACE: &str = "traffic.place";
/// `traffic.snapshot`: every vehicle's lane position, and people in each lane.
pub const SNAPSHOT: &str = "traffic.snapshot";
/// `traffic.junctions`: who may drive into which connector.
pub const JUNCTIONS: &str = "traffic.junctions";
/// `traffic.drive`: the IDM along lanes, junction stops and arrival.
pub const DRIVE: &str = "traffic.drive";

/// Builds the lane graph (logging each problem once) and gives vehicles and junctions their
/// states (at play start).
pub fn insert_traffic(world: &mut World) {
    let graph = build_graph(world);
    let mut by_entity: std::collections::BTreeMap<EntityId, Vec<String>> = Default::default();
    for p in graph.problems() {
        tracing::warn!(entity = %p.entity, "{}", p.message);
        by_entity
            .entry(p.entity)
            .or_default()
            .push(p.message.clone());
    }
    for (id, messages) in by_entity {
        let inserted = world.insert(id, TrafficProblems { messages });
        debug_assert!(inserted.is_ok(), "problems name live entities");
    }
    world.insert_resource(graph);
    world.insert_resource(TrafficSnapshot::default());
    let mut vehicles = Vec::new();
    if let Ok(mut q) = world.query::<&Vehicle>() {
        q.for_each(world, |id, _| vehicles.push(id));
    }
    for id in vehicles {
        if world.get::<VehicleState>(id).is_none() {
            let inserted = world.insert(id, VehicleState::default());
            debug_assert!(inserted.is_ok(), "the entity was just queried");
        }
    }
    let mut junctions = Vec::new();
    if let Ok(mut q) = world.query::<&Junction>() {
        q.for_each(world, |id, _| junctions.push(id));
    }
    for id in junctions {
        if world.get::<JunctionState>(id).is_none() {
            let inserted = world.insert(id, JunctionState::default());
            debug_assert!(inserted.is_ok(), "the entity was just queried");
        }
    }
}

/// The lane graph of `world`'s `Lane` and `Junction` entities.
fn build_graph(world: &mut World) -> TrafficGraph {
    let mut lanes = Vec::new();
    if let Ok(mut q) = world.query::<&Lane>() {
        q.for_each(world, |id, lane| lanes.push((id, lane.clone())));
    }
    let inputs = lanes
        .into_iter()
        .map(|(id, lane)| {
            let (_, rotation, translation) = world_transform(world, id)
                .map(|m| m.to_scale_rotation_translation())
                .unwrap_or_default();
            LaneInput {
                id,
                lane,
                translation,
                rotation,
            }
        })
        .collect();
    let mut junctions = Vec::new();
    if let Ok(mut q) = world.query::<&Junction>() {
        q.for_each(world, |id, j| junctions.push((id, j.clone())));
    }
    TrafficGraph::build(inputs, junctions)
}

/// Adds traffic's processors to `PrePhysics`, after the crowd's `crowd.move`.
pub fn add_traffic_processors(schedule: &mut Schedule) {
    schedule.add_processor(
        processor(PLACE, Phase::PrePhysics)
            .reads::<Vehicle>()
            .reads::<Transform>()
            .writes::<VehicleState>()
            .reads_resource::<TrafficGraph>()
            .after(MOVE)
            .run(place_step),
    );
    schedule.add_processor(
        processor(SNAPSHOT, Phase::PrePhysics)
            .reads::<Vehicle>()
            .reads::<VehicleState>()
            .reads_resource::<TrafficGraph>()
            .reads_resource::<CrowdGrid>()
            .writes_resource::<TrafficSnapshot>()
            .after(PLACE)
            .run(snapshot_step),
    );
    schedule.add_processor(
        processor(JUNCTIONS, Phase::PrePhysics)
            .reads::<Junction>()
            .writes::<JunctionState>()
            .reads_resource::<TrafficGraph>()
            .reads_resource::<TrafficSnapshot>()
            .after(SNAPSHOT)
            .run(junctions_step),
    );
    schedule.add_processor(
        processor(DRIVE, Phase::PrePhysics)
            .reads::<Vehicle>()
            .reads::<JunctionState>()
            .writes::<VehicleState>()
            .writes::<Transform>()
            .reads_resource::<TrafficGraph>()
            .reads_resource::<TrafficSnapshot>()
            .after(JUNCTIONS)
            .run(drive_step),
    );
}

/// `traffic.place`: new vehicles go to their nearest lane; destinations are routed when they
/// change; a bad vehicle or lane fails it.
fn place_step(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let graph = view.resource::<TrafficGraph>().cloned().unwrap_or_default();
    let mut stateless = Vec::new();
    if let Ok(query) = view.query_filtered::<&Vehicle, Without<VehicleState>>() {
        query.for_each(|id, _| stateless.push(id));
    }
    if let Ok(commands) = view.commands() {
        for id in stateless {
            commands.insert(id, VehicleState::default());
        }
    }
    let Ok(query) = view.query::<(&Vehicle, &Transform, &mut VehicleState)>() else {
        return;
    };
    query.par_for_each_deferred(ctx.jobs, CHUNK, |commands, id, (vehicle, t, state)| {
        if let Err(message) = vehicle.check() {
            if state.status != VehicleStatus::Failed {
                tracing::warn!(entity = %id, "{message}");
                state.status = VehicleStatus::Failed;
            }
            return;
        }
        if state.lane.is_none() {
            place(&graph, state, t.translation);
        }
        if let Some(failed) = reroute(&graph, vehicle, state) {
            commands.signal(id, failed);
        }
    });
}

/// Puts `state` on the lane nearest `at` (sideways; ties by lane id).
fn place(graph: &TrafficGraph, state: &mut VehicleState, at: concordia_core::math::Vec3) {
    let mut best: Option<(f32, EntityId, f32)> = None;
    for (id, lane) in graph.lanes() {
        let p = lane.project(at);
        if best.is_none_or(|b| p.off < b.0) {
            best = Some((p.off, id, p.along));
        }
    }
    if let Some((_, lane, along)) = best {
        state.lane = Some(lane);
        state.distance = along;
        state.status = VehicleStatus::Driving;
    }
}

/// Routes `state` when its vehicle's destination changed; the failure, once per destination.
fn reroute(
    graph: &TrafficGraph,
    vehicle: &Vehicle,
    state: &mut VehicleState,
) -> Option<VehicleRouteFailed> {
    if vehicle.destination == state.routed_for {
        return None;
    }
    state.routed_for = vehicle.destination;
    state.route.clear();
    let Some(destination) = vehicle.destination else {
        if state.status == VehicleStatus::Arrived {
            state.status = VehicleStatus::Driving;
        }
        return None;
    };
    match state.lane.and_then(|lane| route(graph, lane, destination)) {
        Some(lanes) => {
            state.route = lanes;
            if matches!(state.status, VehicleStatus::Arrived | VehicleStatus::Failed) {
                state.status = VehicleStatus::Driving;
            }
            None
        }
        None => {
            state.status = VehicleStatus::Failed;
            state.speed = 0.0;
            Some(VehicleRouteFailed { destination })
        }
    }
}

/// `traffic.snapshot`: last tick's vehicles and this tick's people per lane.
fn snapshot_step(view: &mut View<'_>, _: &SystemCtx<'_>) {
    let graph = view.resource::<TrafficGraph>().cloned().unwrap_or_default();
    let grid = view.resource::<CrowdGrid>().ok().cloned();
    let mut vehicles = Vec::new();
    if let Ok(query) = view.query::<(&Vehicle, &VehicleState)>() {
        query.for_each(|id, (v, s)| vehicles.push((id, v.clone(), s.clone())));
    }
    let snapshot = TrafficSnapshot::new(&graph, vehicles, grid.as_ref());
    if let Ok(slot) = view.resource_mut::<TrafficSnapshot>() {
        *slot = snapshot;
    }
}

/// `traffic.junctions`: each junction keeps its grants for vehicles still in it and grants new
/// requests by priority, then waiting time, then id.
fn junctions_step(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let graph = view.resource::<TrafficGraph>().cloned().unwrap_or_default();
    let snapshot = view
        .resource::<TrafficSnapshot>()
        .cloned()
        .unwrap_or_default();
    let tick = ctx.tick.0;
    let Ok(query) = view.query::<(&Junction, &mut JunctionState)>() else {
        return;
    };
    query.par_for_each(ctx.jobs, CHUNK, |id, (_, state)| {
        let Some(info) = graph.junction(id) else {
            state.granted.clear();
            return;
        };
        let held: Vec<Grant> = state
            .granted
            .iter()
            .copied()
            .filter(|g| still_in(&graph, &snapshot, g))
            .collect();
        let mut requests = Vec::new();
        for &approach in &info.approaches {
            for &(_, vehicle) in snapshot.on_lane(approach) {
                let Some(v) = snapshot.vehicle(vehicle) else {
                    continue;
                };
                let Some(lane) = v.next.filter(|n| info.lanes.contains(n)) else {
                    continue;
                };
                let len = graph.lane(approach).map_or(0.0, |l| l.length());
                let left = len - v.along - v.length * 0.5;
                if v.status == VehicleStatus::Failed || left > request_distance(v.desired_gap) {
                    continue;
                }
                requests.push(Request {
                    vehicle,
                    lane,
                    priority: graph.lane(lane).map_or(0, |l| l.priority),
                    waiting_since: match v.status {
                        VehicleStatus::Waiting => v.waiting_since,
                        _ => tick,
                    },
                });
            }
        }
        state.granted = grant(&held, &requests, |a, b| graph.conflict(id, a, b));
    });
}

/// A grant holds while its vehicle is about to enter, is on, or still has its rear in the
/// connector.
fn still_in(graph: &TrafficGraph, snapshot: &TrafficSnapshot, g: &Grant) -> bool {
    let Some(v) = snapshot.vehicle(g.vehicle) else {
        return false;
    };
    if v.lane == g.lane || v.next == Some(g.lane) {
        return true;
    }
    let exits = graph.lane(g.lane).map_or(&[][..], |l| l.next.as_slice());
    exits.contains(&v.lane) && v.along < v.length * 0.5
}

/// `traffic.drive`: every vehicle one tick along its lanes.
fn drive_step(view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let graph = view.resource::<TrafficGraph>().cloned().unwrap_or_default();
    let snapshot = view
        .resource::<TrafficSnapshot>()
        .cloned()
        .unwrap_or_default();
    let mut grants = BTreeSet::new();
    if let Ok(query) = view.query::<&JunctionState>() {
        query.for_each(|_, j| grants.extend(j.granted.iter().map(|g| (g.vehicle, g.lane))));
    }
    let road = Road {
        graph: &graph,
        snapshot: &snapshot,
        grants: &grants,
        tick: ctx.tick.0,
        dt: ctx.dt,
    };
    let Ok(query) = view.query::<(&Vehicle, &mut VehicleState, &mut Transform)>() else {
        return;
    };
    query.par_for_each_deferred(ctx.jobs, CHUNK, |commands, id, (vehicle, state, t)| {
        let driven = drive(&road, id, vehicle, state);
        if let Some((at, dir)) = driven.pose {
            t.translation = at;
            t.rotation = heading(dir);
        }
        if let Some((destination, position)) = driven.arrived {
            commands.signal(
                id,
                VehicleArrived {
                    destination,
                    position,
                },
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use concordia_core::math::{Quat, Vec3};

    use super::*;

    fn id(n: u64) -> EntityId {
        format!("018bcfe5-6800-7000-a424-{n:012x}").parse().unwrap()
    }

    #[test]
    fn grants_hold_until_the_rear_leaves_the_connector() {
        // Connector 1 (x 0..10) leads into exit 2 (x 10..50). Vehicle 9 (4.5 m long) holds a
        // grant for 1. Its rear is still in the connector while its centre is less than 2.25 m
        // into the exit: at 1 m the grant holds; at 3 m it is released. On the connector itself
        // it holds, and on some other lane it doesn't.
        let lane = |n: u64, from: f32, to: f32, next: &[u64]| LaneInput {
            id: id(n),
            lane: Lane {
                points: vec![Vec3::new(from, 0.0, 0.0), Vec3::new(to, 0.0, 0.0)],
                next: next.iter().map(|&k| id(k)).collect(),
                ..Lane::default()
            },
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        };
        let graph = TrafficGraph::build(
            vec![
                lane(1, 0.0, 10.0, &[2]),
                lane(2, 10.0, 50.0, &[]),
                lane(3, 0.0, 10.0, &[]),
            ],
            vec![(id(20), Junction { lanes: vec![id(1)] })],
        );
        let g = Grant {
            vehicle: id(9),
            lane: id(1),
        };
        let at = |lane: u64, distance: f32| {
            let state = VehicleState {
                lane: Some(id(lane)),
                distance,
                status: VehicleStatus::Driving,
                ..VehicleState::default()
            };
            TrafficSnapshot::new(&graph, vec![(id(9), Vehicle::default(), state)], None)
        };
        assert!(
            still_in(&graph, &at(2, 1.0), &g),
            "rear still in the connector"
        );
        assert!(!still_in(&graph, &at(2, 3.0), &g), "clear of it");
        assert!(still_in(&graph, &at(1, 5.0), &g), "on it");
        assert!(!still_in(&graph, &at(3, 5.0), &g), "elsewhere");
    }
}
