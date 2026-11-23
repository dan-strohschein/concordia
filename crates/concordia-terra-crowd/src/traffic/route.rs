//! Routes over the lane graph (Dijkstra 1959, by travel time) and wandering choices.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use concordia_core::id::EntityId;
use concordia_terra::splitmix64;

use super::graph::TrafficGraph;

/// The quickest lanes from the end of `from` to the end of `to` (travel time = length / speed
/// limit; ties by lane id), `to` included and `from` not; empty when they are the same lane.
pub(crate) fn route(graph: &TrafficGraph, from: EntityId, to: EntityId) -> Option<Vec<EntityId>> {
    graph.lane(from)?;
    graph.lane(to)?;
    if from == to {
        return Some(Vec::new());
    }
    let mut best: BTreeMap<EntityId, (f32, EntityId)> = BTreeMap::new();
    let mut open = BinaryHeap::new();
    open.push(Visit(0.0, from));
    let mut done = BTreeSet::new();
    while let Some(Visit(cost, at)) = open.pop() {
        if !done.insert(at) {
            continue;
        }
        if at == to {
            return Some(path_to(&best, from, to));
        }
        let Some(lane) = graph.lane(at) else {
            continue;
        };
        for &n in &lane.next {
            let Some(next) = graph.lane(n) else {
                continue;
            };
            let total = cost + next.length() / next.speed_limit;
            if best.get(&n).is_none_or(|(c, _)| total < *c) {
                best.insert(n, (total, at));
                open.push(Visit(total, n));
            }
        }
    }
    None
}

/// The lanes after `from` up to `to`, walking the predecessors back.
fn path_to(
    best: &BTreeMap<EntityId, (f32, EntityId)>,
    from: EntityId,
    to: EntityId,
) -> Vec<EntityId> {
    let mut path = vec![to];
    let mut at = to;
    while let Some(&(_, prev)) = best.get(&at) {
        if prev == from {
            break;
        }
        path.push(prev);
        at = prev;
    }
    path.reverse();
    path
}

/// A lane to visit at a cost: the cheapest first, then the lowest id.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Visit(f32, EntityId);

impl Eq for Visit {}

impl Ord for Visit {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .total_cmp(&self.0)
            .then_with(|| other.1.cmp(&self.1))
    }
}

impl PartialOrd for Visit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The lane a wandering vehicle takes after `lane`: a fixed choice among `next` by its ids.
pub(crate) fn wander(vehicle: EntityId, lane: EntityId, next: &[EntityId]) -> Option<EntityId> {
    if next.is_empty() {
        return None;
    }
    let h = splitmix64(fold(vehicle) ^ splitmix64(fold(lane)));
    next.get((h % next.len() as u64) as usize).copied()
}

/// An id's two 64-bit halves XORed.
fn fold(id: EntityId) -> u64 {
    let v = id.as_uuid().as_u128();
    (v as u64) ^ ((v >> 64) as u64)
}

#[cfg(test)]
mod tests {
    use concordia_core::math::{Quat, Vec3};

    use super::*;
    use crate::traffic::components::Lane;
    use crate::traffic::graph::LaneInput;

    fn id(n: u64) -> EntityId {
        format!("018bcfe5-6800-7000-a424-{n:012x}").parse().unwrap()
    }

    /// A straight lane `length` m long at `speed`, leading to `next`.
    fn lane(n: u64, length: f32, speed: f32, next: &[u64]) -> LaneInput {
        LaneInput {
            id: id(n),
            lane: Lane {
                points: vec![Vec3::ZERO, Vec3::new(length, 0.0, 0.0)],
                speed_limit: speed,
                next: next.iter().map(|&k| id(k)).collect(),
                ..Lane::default()
            },
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        }
    }

    #[test]
    fn routes_take_the_quickest_lanes() {
        // From A (1) to D (4) via B (2: 100 m at 10 m/s, 10 s) or C (3: 50 m at 2.5 m/s, 20 s);
        // D is 20 m at 10 m/s either way. B is quicker: [B, D].
        let graph = TrafficGraph::build(
            vec![
                lane(1, 10.0, 10.0, &[3, 2]),
                lane(2, 100.0, 10.0, &[4]),
                lane(3, 50.0, 2.5, &[4]),
                lane(4, 20.0, 10.0, &[]),
            ],
            vec![],
        );
        assert_eq!(route(&graph, id(1), id(4)), Some(vec![id(2), id(4)]));
        assert_eq!(route(&graph, id(1), id(1)), Some(vec![]), "already there");
        assert_eq!(route(&graph, id(4), id(1)), None, "no way back");
        // A tie (C now 50 m at 5 m/s: 10 s) goes to the lower id, B.
        let graph = TrafficGraph::build(
            vec![
                lane(1, 10.0, 10.0, &[3, 2]),
                lane(2, 100.0, 10.0, &[4]),
                lane(3, 50.0, 5.0, &[4]),
                lane(4, 20.0, 10.0, &[]),
            ],
            vec![],
        );
        assert_eq!(route(&graph, id(1), id(4)), Some(vec![id(2), id(4)]));
    }

    #[test]
    fn wander_choices_are_fixed_by_ids() {
        // The choice depends only on the vehicle and the lane: the same every time, none at a
        // dead end, the only lane when there is one, and both branches of a fork used by 64
        // vehicles.
        let fork = [id(2), id(3)];
        let first = wander(id(100), id(1), &fork);
        assert!(first.is_some());
        assert_eq!(wander(id(100), id(1), &fork), first);
        assert_eq!(wander(id(100), id(1), &[]), None);
        assert_eq!(wander(id(100), id(1), &[id(2)]), Some(id(2)));
        let picks: std::collections::BTreeSet<_> = (0..64)
            .filter_map(|v| wander(id(200 + v), id(1), &fork))
            .collect();
        assert_eq!(picks.len(), 2, "both branches");
    }
}
