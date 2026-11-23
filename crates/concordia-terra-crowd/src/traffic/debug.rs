//! The `lanes` debug layer: lanes with direction arrows, connectors by state, vehicle footprints,
//! waiting vehicles' stop lines and problems in red.

use std::collections::BTreeSet;

use concordia_api::{DebugGeometry, DebugLine};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_scene::builtin::{Transform, world_transform};
use concordia_terra::World;

use super::components::{JunctionState, Lane, Vehicle, VehicleState, VehicleStatus};
use super::graph::TrafficGraph;

/// Lines sit this far above the lanes, as nav's layers.
const LIFT: f32 = 0.02;
/// Meters between direction arrows.
const ARROW_EVERY: f32 = 5.0;
/// Vehicles are drawn this wide.
const WIDTH: f32 = 1.8;
const LANE: [f32; 4] = [0.85, 0.85, 0.9, 1.0];
const FREE: [f32; 4] = [0.3, 0.9, 0.4, 1.0];
const GRANTED: [f32; 4] = [1.0, 0.85, 0.1, 1.0];
const OCCUPIED: [f32; 4] = [1.0, 0.5, 0.1, 1.0];
const VEHICLE: [f32; 4] = [0.3, 0.7, 1.0, 1.0];
const STOP: [f32; 4] = [1.0, 0.3, 0.8, 1.0];
const PROBLEM: [f32; 4] = [1.0, 0.1, 0.1, 1.0];

/// The lanes layer of a world that is driving (none without a lane graph).
pub(crate) fn lanes_geometry(world: &World) -> DebugGeometry {
    let mut out = DebugGeometry::default();
    let Some(graph) = world.resource::<TrafficGraph>() else {
        return out;
    };
    let ids = world.entity_ids_sorted();
    let (granted, occupied) = connector_states(world, &ids);
    for (id, lane) in graph.lanes() {
        let color = match lane.junction {
            None => LANE,
            Some(_) if occupied.contains(&id) => OCCUPIED,
            Some(_) if granted.contains(&id) => GRANTED,
            Some(_) => FREE,
        };
        for w in lane.points.windows(2) {
            line(&mut out, w[0], w[1], color);
        }
        let mut s = ARROW_EVERY;
        while s < lane.length() {
            let (p, dir) = lane.at(s);
            let side = Vec3::new(-dir.z, 0.0, dir.x) * 0.4;
            line(&mut out, p - dir * 0.6 + side, p, color);
            line(&mut out, p - dir * 0.6 - side, p, color);
            s += ARROW_EVERY;
        }
    }
    for &id in &ids {
        vehicle(&mut out, world, graph, id);
    }
    problems(&mut out, world, graph);
    out
}

/// Connectors granted to someone, and connectors a vehicle is on.
fn connector_states(world: &World, ids: &[EntityId]) -> (BTreeSet<EntityId>, BTreeSet<EntityId>) {
    let mut granted = BTreeSet::new();
    let mut occupied = BTreeSet::new();
    for &id in ids {
        if let Some(j) = world.get::<JunctionState>(id) {
            granted.extend(j.granted.iter().map(|g| g.lane));
        }
        if let Some(lane) = world.get::<VehicleState>(id).and_then(|s| s.lane) {
            occupied.insert(lane);
        }
    }
    (granted, occupied)
}

/// A vehicle's footprint, and a line from its front to the stop line it waits at.
fn vehicle(out: &mut DebugGeometry, world: &World, graph: &TrafficGraph, id: EntityId) {
    let (Some(v), Some(state), Some(t)) = (
        world.get::<Vehicle>(id),
        world.get::<VehicleState>(id),
        world.get::<Transform>(id),
    ) else {
        return;
    };
    let dir = t.rotation * Vec3::Z;
    let side = Vec3::new(-dir.z, 0.0, dir.x) * (WIDTH * 0.5);
    let ahead = dir * (v.length * 0.5);
    let c = t.translation;
    let corners = [
        c + ahead + side,
        c + ahead - side,
        c - ahead - side,
        c - ahead + side,
    ];
    for k in 0..4 {
        line(out, corners[k], corners[(k + 1) % 4], VEHICLE);
    }
    if state.status == VehicleStatus::Waiting
        && let Some(lane) = state.lane.and_then(|l| graph.lane(l))
    {
        let (end, _) = lane.at(lane.length());
        line(out, c + ahead, end, STOP);
    }
}

/// A red cross at each problem lane's first point (or a problem junction's position).
fn problems(out: &mut DebugGeometry, world: &World, graph: &TrafficGraph) {
    let drawn: BTreeSet<EntityId> = graph.problems().iter().map(|p| p.entity).collect();
    for id in drawn {
        let m = world_transform(world, id).unwrap_or_default();
        let local = world
            .get::<Lane>(id)
            .and_then(|l| l.points.first().copied())
            .unwrap_or(Vec3::ZERO);
        let at = m.transform_point3(local);
        let (x, z) = (Vec3::new(0.5, 0.0, 0.5), Vec3::new(0.5, 0.0, -0.5));
        line(out, at - x, at + x, PROBLEM);
        line(out, at - z, at + z, PROBLEM);
    }
}

fn line(out: &mut DebugGeometry, from: Vec3, to: Vec3, color: [f32; 4]) {
    out.lines.push(DebugLine {
        from: from + Vec3::Y * LIFT,
        to: to + Vec3::Y * LIFT,
        color,
    });
}
