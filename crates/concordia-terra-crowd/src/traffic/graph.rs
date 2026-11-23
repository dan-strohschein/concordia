//! The lane graph (`TrafficGraph`): lanes in world space with their lengths, junctions with their
//! conflicts, and the problems found building it. A resource built at play start from the `Lane`
//! and `Junction` entities; derived data, so not hashed.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};

use super::components::{Junction, Lane};

/// One lane, ready to drive.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneInfo {
    /// World-space points, in driving order.
    pub points: Vec<Vec3>,
    /// Distance along the lane to each point (the first is 0).
    pub lengths: Vec<f32>,
    /// Its speed limit.
    pub speed_limit: f32,
    /// Its width.
    pub width: f32,
    /// The lanes it leads to that exist.
    pub next: Vec<EntityId>,
    /// Its priority at a junction.
    pub priority: u8,
    /// The junction it is a connector of.
    pub junction: Option<EntityId>,
}

/// Where a point is relative to a lane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    /// Distance along the lane to the closest point.
    pub along: f32,
    /// Sideways distance from the lane to the point.
    pub off: f32,
    /// The point's height above the lane there.
    pub up: f32,
}

/// One junction's connectors and which pairs conflict.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JunctionInfo {
    /// Its connectors, by id.
    pub lanes: Vec<EntityId>,
    /// Conflicting pairs (lower id first).
    pub conflicts: BTreeSet<(EntityId, EntityId)>,
    /// The lanes leading into its connectors, by id.
    pub approaches: Vec<EntityId>,
}

/// Something wrong with a lane or junction: logged once and drawn red; it never stops play.
#[derive(Debug, Clone, PartialEq)]
pub struct TrafficProblem {
    /// The lane or junction.
    pub entity: EntityId,
    /// What is wrong, naming the component and field.
    pub message: String,
}

/// The lane graph.
#[derive(Debug, Clone, Default)]
pub struct TrafficGraph {
    lanes: Arc<BTreeMap<EntityId, LaneInfo>>,
    junctions: Arc<BTreeMap<EntityId, JunctionInfo>>,
    problems: Arc<Vec<TrafficProblem>>,
}

/// A lane entity as the graph is built from it.
#[derive(Debug, Clone)]
pub struct LaneInput {
    /// The entity.
    pub id: EntityId,
    /// Its lane.
    pub lane: Lane,
    /// Its world translation.
    pub translation: Vec3,
    /// Its world rotation (scale never applies).
    pub rotation: Quat,
}

impl LaneInfo {
    /// Its length.
    pub fn length(&self) -> f32 {
        self.lengths.last().copied().unwrap_or(0.0)
    }

    /// The point `along` meters in (clamped to the lane) and the direction there.
    pub fn at(&self, along: f32) -> (Vec3, Vec3) {
        let along = along.clamp(0.0, self.length());
        let last = self.points.len().saturating_sub(2);
        let i = (0..=last)
            .find(|&i| along <= self.lengths[i + 1] && self.lengths[i + 1] > self.lengths[i])
            .unwrap_or(last);
        let (a, b) = (self.points[i], self.points[i + 1]);
        let span = self.lengths[i + 1] - self.lengths[i];
        let t = match span > 0.0 {
            true => (along - self.lengths[i]) / span,
            false => 0.0,
        };
        (a + (b - a) * t, (b - a).normalize_or_zero())
    }

    /// The closest point of the lane to `p` (sideways), and how far `p` is from it.
    pub fn project(&self, p: Vec3) -> Projection {
        let mut best = Projection {
            along: 0.0,
            off: f32::MAX,
            up: 0.0,
        };
        for i in 0..self.points.len().saturating_sub(1) {
            let (a, b) = (self.points[i], self.points[i + 1]);
            let (dx, dz) = (b.x - a.x, b.z - a.z);
            let d2 = dx * dx + dz * dz;
            let t = match d2 > 0.0 {
                true => (((p.x - a.x) * dx + (p.z - a.z) * dz) / d2).clamp(0.0, 1.0),
                false => 0.0,
            };
            let on = a + (b - a) * t;
            let off = ((p.x - on.x).powi(2) + (p.z - on.z).powi(2)).sqrt();
            if off < best.off {
                best = Projection {
                    along: self.lengths[i] + (self.lengths[i + 1] - self.lengths[i]) * t,
                    off,
                    up: p.y - on.y,
                };
            }
        }
        best
    }
}

impl TrafficGraph {
    /// Builds the graph from lane and junction entities (in any order).
    pub fn build(lanes: Vec<LaneInput>, junctions: Vec<(EntityId, Junction)>) -> Self {
        let mut graph = Self::default();
        let mut inputs = lanes;
        inputs.sort_by_key(|l| l.id);
        let exists: BTreeSet<EntityId> = inputs.iter().map(|l| l.id).collect();
        for input in &inputs {
            match place(input) {
                Ok(info) => {
                    Arc::make_mut(&mut graph.lanes).insert(input.id, info);
                }
                Err(message) => graph.problem(input.id, message),
            }
        }
        for input in &inputs {
            for n in &input.lane.next {
                if !exists.contains(n) {
                    graph.problem(input.id, format!("Lane.next names {n}, which isn't a lane"));
                }
            }
        }
        let valid: BTreeSet<EntityId> = graph.lanes.keys().copied().collect();
        for info in Arc::make_mut(&mut graph.lanes).values_mut() {
            info.next.retain(|n| valid.contains(n));
        }
        graph.add_junctions(junctions, &exists);
        graph
    }

    fn add_junctions(
        &mut self,
        mut junctions: Vec<(EntityId, Junction)>,
        exists: &BTreeSet<EntityId>,
    ) {
        junctions.sort_by_key(|(id, _)| *id);
        let mut claimed: BTreeMap<EntityId, EntityId> = BTreeMap::new();
        for (id, junction) in junctions {
            let mut lanes = Vec::new();
            for &lane in &junction.lanes {
                if !exists.contains(&lane) {
                    self.problem(
                        id,
                        format!("Junction.lanes names {lane}, which isn't a lane"),
                    );
                } else if let Some(other) = claimed.get(&lane) {
                    self.problem(
                        id,
                        format!("Junction.lanes: {lane} is already a connector of {other}"),
                    );
                } else {
                    claimed.insert(lane, id);
                    if self.lanes.contains_key(&lane) {
                        lanes.push(lane);
                    }
                }
            }
            lanes.sort();
            lanes.dedup();
            for lane in &lanes {
                if let Some(info) = Arc::make_mut(&mut self.lanes).get_mut(lane) {
                    info.junction = Some(id);
                }
            }
            let conflicts = self.conflicts_among(&lanes);
            let approaches = self
                .lanes
                .iter()
                .filter(|(l, info)| {
                    !lanes.contains(l) && info.next.iter().any(|n| lanes.contains(n))
                })
                .map(|(l, _)| *l)
                .collect();
            Arc::make_mut(&mut self.junctions).insert(
                id,
                JunctionInfo {
                    lanes,
                    conflicts,
                    approaches,
                },
            );
        }
    }

    /// Pairs of `lanes` that cross (away from a shared end) or lead into a common lane.
    fn conflicts_among(&self, lanes: &[EntityId]) -> BTreeSet<(EntityId, EntityId)> {
        let mut out = BTreeSet::new();
        for (i, &a) in lanes.iter().enumerate() {
            for &b in &lanes[i + 1..] {
                let (la, lb) = (&self.lanes[&a], &self.lanes[&b]);
                let merge = la.next.iter().any(|n| lb.next.contains(n));
                if merge || crosses(la, lb) {
                    out.insert((a.min(b), a.max(b)));
                }
            }
        }
        out
    }

    fn problem(&mut self, entity: EntityId, message: String) {
        Arc::make_mut(&mut self.problems).push(TrafficProblem { entity, message });
    }

    /// The lane `id`, if it is valid.
    pub fn lane(&self, id: EntityId) -> Option<&LaneInfo> {
        self.lanes.get(&id)
    }

    /// Every valid lane, by id.
    pub fn lanes(&self) -> impl Iterator<Item = (EntityId, &LaneInfo)> {
        self.lanes.iter().map(|(id, l)| (*id, l))
    }

    /// The junction `id`, if it is valid.
    pub fn junction(&self, id: EntityId) -> Option<&JunctionInfo> {
        self.junctions.get(&id)
    }

    /// Every valid junction, by id.
    pub fn junctions(&self) -> impl Iterator<Item = (EntityId, &JunctionInfo)> {
        self.junctions.iter().map(|(id, j)| (*id, j))
    }

    /// Whether connectors `a` and `b` of one junction conflict.
    pub fn conflict(&self, junction: EntityId, a: EntityId, b: EntityId) -> bool {
        let key = (a.min(b), a.max(b));
        self.junctions
            .get(&junction)
            .is_some_and(|j| j.conflicts.contains(&key))
    }

    /// What was wrong, by entity.
    pub fn problems(&self) -> &[TrafficProblem] {
        &self.problems
    }
}

/// A lane in world space, or what is wrong with it.
fn place(input: &LaneInput) -> Result<LaneInfo, String> {
    input.lane.check()?;
    let points: Vec<Vec3> = input
        .lane
        .points
        .iter()
        .map(|p| input.translation + input.rotation * *p)
        .collect();
    let mut lengths = Vec::with_capacity(points.len());
    let mut total = 0.0f32;
    lengths.push(0.0);
    for w in points.windows(2) {
        total += (w[1] - w[0]).length();
        lengths.push(total);
    }
    if total.is_nan() || total <= 0.0 {
        return Err("Lane.points have zero length".to_owned());
    }
    Ok(LaneInfo {
        points,
        lengths,
        speed_limit: input.lane.speed_limit,
        width: input.lane.width,
        next: input.lane.next.clone(),
        priority: input.lane.priority,
        junction: None,
    })
}

/// Tolerance for where two lanes touch, as a fraction of a segment.
const TOUCH: f32 = 1e-4;

/// Whether `a` and `b` cross sideways anywhere except where both are at one of their ends.
fn crosses(a: &LaneInfo, b: &LaneInfo) -> bool {
    let (na, nb) = (a.points.len() - 1, b.points.len() - 1);
    for i in 0..na {
        for j in 0..nb {
            let Some((t, u)) =
                segments_meet(a.points[i], a.points[i + 1], b.points[j], b.points[j + 1])
            else {
                continue;
            };
            let end_a = (i == 0 && t <= TOUCH) || (i == na - 1 && t >= 1.0 - TOUCH);
            let end_b = (j == 0 && u <= TOUCH) || (j == nb - 1 && u >= 1.0 - TOUCH);
            if !(end_a && end_b) {
                return true;
            }
        }
    }
    false
}

/// Where segments p1–p2 and q1–q2 meet sideways, as fractions along each (parallel ones don't).
fn segments_meet(p1: Vec3, p2: Vec3, q1: Vec3, q2: Vec3) -> Option<(f32, f32)> {
    let cross = |ax: f32, az: f32, bx: f32, bz: f32| ax * bz - az * bx;
    let (rx, rz) = (p2.x - p1.x, p2.z - p1.z);
    let (sx, sz) = (q2.x - q1.x, q2.z - q1.z);
    let d = cross(rx, rz, sx, sz);
    if d.abs() < 1e-9 {
        return None;
    }
    let (qx, qz) = (q1.x - p1.x, q1.z - p1.z);
    let t = cross(qx, qz, sx, sz) / d;
    let u = cross(qx, qz, rx, rz) / d;
    let inside = |x: f32| (-TOUCH..=1.0 + TOUCH).contains(&x);
    (inside(t) && inside(u)).then_some((t, u))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn id(n: u64) -> EntityId {
        format!("018bcfe5-6800-7000-a424-{n:012x}").parse().unwrap()
    }

    fn lane(n: u64, points: &[[f32; 2]], next: &[u64]) -> LaneInput {
        LaneInput {
            id: id(n),
            lane: Lane {
                points: points.iter().map(|p| Vec3::new(p[0], 0.0, p[1])).collect(),
                next: next.iter().map(|&k| id(k)).collect(),
                ..Lane::default()
            },
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        }
    }

    #[test]
    fn lanes_are_placed_measured_and_projected() {
        // An L: (0, 0) → (10, 0) → (10, 5), so 15 m long. Placed with its entity at (100, 0, 0)
        // turned 90° about +y (x → −z), its points are (100, 0, 0), (100, 0, −10), (105, 0, −10).
        let mut l = lane(1, &[[0.0, 0.0], [10.0, 0.0], [10.0, 5.0]], &[]);
        l.translation = Vec3::new(100.0, 0.0, 0.0);
        l.rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let graph = TrafficGraph::build(vec![l], vec![]);
        let info = graph.lane(id(1)).expect("valid");
        assert!((info.length() - 15.0).abs() < 1e-4);
        let (p, dir) = info.at(12.0);
        assert!((p - Vec3::new(102.0, 0.0, -10.0)).length() < 1e-4, "{p}");
        assert!((dir - Vec3::X).length() < 1e-4, "{dir}");
        let (start, first) = info.at(-3.0);
        assert!(
            (start - Vec3::new(100.0, 0.0, 0.0)).length() < 1e-4,
            "clamped: {start}"
        );
        assert!((first - Vec3::NEG_Z).length() < 1e-4, "{first}");
        // (101, 0.5, −4) is 1 m beside the first leg, 4 m along, 0.5 m up.
        let near = info.project(Vec3::new(101.0, 0.5, -4.0));
        assert!((near.along - 4.0).abs() < 1e-4 && (near.off - 1.0).abs() < 1e-4);
        assert!((near.up - 0.5).abs() < 1e-4);
    }

    #[test]
    fn junction_conflicts_are_crossings_and_merges() {
        // Connectors of one junction: 1 runs north–south through (0, 0); 2 west–east through it
        // (they cross); 3 and 4 both lead into lane 9 (a merge); 5 and 6 leave the same point
        // (0, 5) for different lanes (a diverge: no conflict); 7 runs parallel to 1, 3 m east,
        // and touches nothing.
        let lanes = vec![
            lane(1, &[[0.0, 5.0], [0.0, -5.0]], &[]),
            lane(2, &[[-5.0, 0.0], [5.0, 0.0]], &[]),
            lane(3, &[[-5.0, 20.0], [0.0, 25.0]], &[9]),
            lane(4, &[[5.0, 20.0], [0.0, 25.0]], &[9]),
            lane(5, &[[0.0, 5.0], [-5.0, 10.0]], &[]),
            lane(6, &[[0.0, 5.0], [5.0, 10.0]], &[]),
            lane(7, &[[3.0, 5.0], [3.0, -1.0]], &[]),
            lane(9, &[[0.0, 25.0], [0.0, 35.0]], &[]),
        ];
        let connectors = [1, 2, 3, 4, 5, 6, 7].map(id).to_vec();
        let graph = TrafficGraph::build(lanes, vec![(id(20), Junction { lanes: connectors })]);
        assert!(graph.problems().is_empty(), "{:?}", graph.problems());
        let j = id(20);
        assert!(graph.conflict(j, id(1), id(2)), "crossing");
        assert!(graph.conflict(j, id(2), id(1)), "either order");
        assert!(graph.conflict(j, id(3), id(4)), "merging into 9");
        assert!(!graph.conflict(j, id(5), id(6)), "a shared start only");
        assert!(!graph.conflict(j, id(1), id(7)), "parallel");
        assert!(graph.conflict(j, id(2), id(7)), "2 crosses 7 at (3, 0)");
        assert_eq!(graph.lane(id(1)).unwrap().junction, Some(j));
        assert_eq!(graph.lane(id(9)).unwrap().junction, None);
    }

    #[test]
    fn lane_problems_name_the_entity_and_field() {
        // Lane 1 leads to 2 (fine) and to 8, which isn't a lane; lane 3 has one point; lane 4
        // has zero length; junction 20 lists lane 2 and a missing 7; junction 21 lists 2 again.
        let mut lanes = vec![
            lane(1, &[[0.0, 0.0], [10.0, 0.0]], &[2, 8]),
            lane(2, &[[10.0, 0.0], [20.0, 0.0]], &[]),
            lane(3, &[[0.0, 0.0]], &[]),
            lane(4, &[[1.0, 1.0], [1.0, 1.0]], &[]),
        ];
        lanes[1].lane.speed_limit = 0.0;
        let junctions = vec![
            (
                id(20),
                Junction {
                    lanes: vec![id(2), id(7)],
                },
            ),
            (id(21), Junction { lanes: vec![id(2)] }),
        ];
        let graph = TrafficGraph::build(lanes, junctions);
        let said = |n: u64| -> Vec<String> {
            graph
                .problems()
                .iter()
                .filter(|p| p.entity == id(n))
                .map(|p| p.message.clone())
                .collect()
        };
        assert!(
            said(1)
                .iter()
                .any(|m| m.contains("Lane.next") && m.contains(&id(8).to_string()))
        );
        assert!(
            said(2).iter().any(|m| m.contains("Lane.speed_limit")),
            "{:?}",
            said(2)
        );
        assert!(said(3).iter().any(|m| m.contains("Lane.points")));
        assert!(said(4).iter().any(|m| m.contains("zero length")));
        assert!(
            said(20)
                .iter()
                .any(|m| m.contains("Junction.lanes") && m.contains(&id(7).to_string()))
        );
        assert!(
            said(21).iter().any(|m| m.contains("already")),
            "{:?}",
            said(21)
        );
        // Bad lanes are left out, and links to them dropped: 1 keeps no `next` (2 is bad, 8 isn't
        // a lane).
        assert!(graph.lane(id(2)).is_none() && graph.lane(id(3)).is_none());
        assert_eq!(graph.lane(id(1)).unwrap().next, Vec::<EntityId>::new());
    }
}
