//! `TrafficSnapshot`: every vehicle's place on the graph and the people standing in each lane, at
//! the start of traffic's step (last tick's state). A resource rebuilt each tick from hashed state,
//! so it isn't hashed; its data is shared, so cloning it is cheap.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use concordia_core::id::EntityId;

use super::components::{Vehicle, VehicleState, VehicleStatus};
use super::graph::TrafficGraph;
use super::route::wander;
use crate::grid::CrowdGrid;

/// People more than this far above or below a lane aren't in it, in meters.
pub(crate) const PEOPLE_HEIGHT: f32 = 2.0;

/// One vehicle in the snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleSnap {
    /// Its lane.
    pub lane: EntityId,
    /// Meters along it, to its centre.
    pub along: f32,
    /// Its speed.
    pub speed: f32,
    /// Its length.
    pub length: f32,
    /// The lane it drives into next (none: it stops at this lane's end).
    pub next: Option<EntityId>,
    /// What it was doing.
    pub status: VehicleStatus,
    /// The tick it started waiting.
    pub waiting_since: u64,
    /// The IDM's desired gap at its speed, s₀ + vT + v² / (2√(ab)).
    pub desired_gap: f32,
}

#[derive(Debug, Default)]
struct Inner {
    vehicles: BTreeMap<EntityId, VehicleSnap>,
    on_lane: HashMap<EntityId, Vec<(f32, EntityId)>>,
    people: HashMap<EntityId, Vec<(f32, f32)>>,
}

/// Every vehicle on the graph, and the people in each lane.
#[derive(Debug, Clone, Default)]
pub struct TrafficSnapshot(Arc<Inner>);

impl TrafficSnapshot {
    /// The snapshot of `vehicles` on `graph`, with the people `grid` holds.
    pub(crate) fn new(
        graph: &TrafficGraph,
        vehicles: Vec<(EntityId, Vehicle, VehicleState)>,
        grid: Option<&CrowdGrid>,
    ) -> Self {
        let mut inner = Inner::default();
        for (id, vehicle, state) in vehicles {
            let Some(lane) = state.lane.filter(|l| graph.lane(*l).is_some()) else {
                continue;
            };
            let v = state.speed;
            let desired_gap = vehicle.min_gap
                + v * vehicle.time_headway
                + v * v / (2.0 * (vehicle.max_acceleration * vehicle.comfortable_braking).sqrt());
            let snap = VehicleSnap {
                lane,
                along: state.distance,
                speed: v,
                length: vehicle.length,
                next: next_lane(graph, id, &state),
                status: state.status,
                waiting_since: state.waiting_since,
                desired_gap,
            };
            inner
                .on_lane
                .entry(lane)
                .or_default()
                .push((snap.along, id));
            inner.vehicles.insert(id, snap);
        }
        for list in inner.on_lane.values_mut() {
            list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        }
        if let Some(grid) = grid {
            inner.people = people(graph, grid);
        }
        Self(Arc::new(inner))
    }

    /// Vehicle `id`.
    pub fn vehicle(&self, id: EntityId) -> Option<&VehicleSnap> {
        self.0.vehicles.get(&id)
    }

    /// Every vehicle, by id.
    pub fn vehicles(&self) -> impl Iterator<Item = (EntityId, &VehicleSnap)> {
        self.0.vehicles.iter().map(|(id, v)| (*id, v))
    }

    /// The vehicles on `lane`, by (distance, id).
    pub fn on_lane(&self, lane: EntityId) -> &[(f32, EntityId)] {
        self.0.on_lane.get(&lane).map_or(&[], Vec::as_slice)
    }

    /// The people in `lane`, as (distance along it, radius), nearest the start first.
    pub fn people(&self, lane: EntityId) -> &[(f32, f32)] {
        self.0.people.get(&lane).map_or(&[], Vec::as_slice)
    }
}

/// The lane a vehicle drives into after its own: its route's next, or a wandering choice.
pub(crate) fn next_lane(
    graph: &TrafficGraph,
    id: EntityId,
    state: &VehicleState,
) -> Option<EntityId> {
    let lane = state.lane?;
    match state.routed_for {
        Some(_) => state.route.first().copied(),
        None => wander(id, lane, &graph.lane(lane)?.next),
    }
}

/// Everyone in the grid whose feet are within width / 2 + their radius of a lane (sideways) and
/// `PEOPLE_HEIGHT` of it vertically, per lane.
fn people(graph: &TrafficGraph, grid: &CrowdGrid) -> HashMap<EntityId, Vec<(f32, f32)>> {
    let mut out = HashMap::new();
    let widest = grid
        .entries()
        .iter()
        .map(|e| e.radius)
        .fold(0.0f32, f32::max);
    for (id, lane) in graph.lanes() {
        let reach = lane.width * 0.5 + widest;
        let mut seen = BTreeSet::new();
        for w in lane.points.windows(2) {
            let mid = (w[0] + w[1]) * 0.5;
            let half = (w[1] - w[0]).length() * 0.5;
            seen.extend(grid.around(mid, half + reach).map(|e| e.id));
        }
        let mut found: Vec<(f32, f32)> = seen
            .into_iter()
            .filter_map(|p| grid.get(p))
            .filter_map(|e| {
                let at = lane.project(e.position);
                let inside = at.off <= lane.width * 0.5 + e.radius && at.up.abs() <= PEOPLE_HEIGHT;
                inside.then_some((at.along, e.radius))
            })
            .collect();
        if !found.is_empty() {
            found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            out.insert(id, found);
        }
    }
    out
}
