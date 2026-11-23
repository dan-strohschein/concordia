//! Lane traffic (2.5d) on the rig without physics: vehicles move themselves along their lanes, so
//! every tick is hand-computable.
//!
//! **Ticks** run at 60 Hz; the first step is tick 0. Each tick traffic places new vehicles, takes
//! its snapshot, grants junctions, then drives every vehicle once.
//!
//! **The IDM** with the default vehicle (a 1, b 1.5, T 1.5 s, s₀ 2 m, 4.5 m long): on a free road
//! the acceleration is a [1 − (v / v₀)⁴], and the ballistic update moves v·dt + ½a·dt² and adds
//! a·dt. From rest that is 1 m/s² at first: after tick 0, v = 1/60 and the vehicle has moved
//! 0.5 / 3600 m; after tick 1, v = 2/60 (less 4 × 10⁻¹¹) and it has moved 2 / 3600 m in all.
//!
//! **Stopping:** what a vehicle must stop for (a stop line, a dead end, its destination's end) is a
//! stopped obstacle s₀ past the line, so it stops with its front at the line; a person is one at
//! the person's near edge, so it stops s₀ short of them.

mod common;

use common::{Level, Rig, sideways};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_terra_crowd::{
    JunctionState, TrafficGraph, TrafficProblems, Vehicle, VehicleArrived, VehicleRouteFailed,
    VehicleStatus,
};

fn car() -> Vehicle {
    Vehicle::default()
}

fn going_to(lane: EntityId) -> Vehicle {
    Vehicle {
        destination: Some(lane),
        ..car()
    }
}

#[test]
fn vehicle_accelerates_to_the_limit_along_its_lane() {
    // A 1 km lane along +x at 10 m/s; the vehicle (top speed 15) spawns at (5, 0, 0.4) and is
    // placed on the lane at 5 m. v₀ = min(15, 10) = 10. The header's first two ticks give
    // x = 5 + 0.5 / 3600 and 5 + 2 / 3600. On a free road dv/dt = 1 − (v / 10)⁴, so the time to
    // reach v is 10 ∫ dr / (1 − r⁴) = 5 (atanh r + atan r) with r = v / 10. At 60 s,
    // atanh r = 12 − atan r ≈ 11.2, so 1 − r ≈ 2e^(−22.4), far under f32's precision: the speed
    // is 10 within 1e-3. The lane's end is over 150 m away (outside the look-ahead) until then.
    // It faces along the lane: its +z turns to +x.
    let mut level = Level::new();
    level.lane(&[[0.0, 0.0], [1000.0, 0.0]], 10.0);
    let v = level.vehicle(Vec3::new(5.0, 0.0, 0.4), car());
    let mut rig = Rig::new(level);
    rig.step(1);
    let s = rig.vehicle(v);
    assert_eq!(s.status, VehicleStatus::Driving);
    assert!((s.speed - 1.0 / 60.0).abs() < 1e-6, "{s:?}");
    assert!(
        (rig.pos(v).x - (5.0 + 0.5 / 3600.0)).abs() < 1e-5,
        "{}",
        rig.pos(v)
    );
    assert!(rig.pos(v).z.abs() < 1e-6, "on the lane: {}", rig.pos(v));
    rig.step(1);
    assert!((rig.pos(v).x - (5.0 + 2.0 / 3600.0)).abs() < 1e-5);
    rig.step(3598);
    assert!(
        (rig.vehicle(v).speed - 10.0).abs() < 1e-3,
        "{:?}",
        rig.vehicle(v)
    );
    let forward = rig
        .world
        .get::<concordia_scene::builtin::Transform>(v)
        .unwrap()
        .rotation
        * Vec3::Z;
    assert!((forward - Vec3::X).length() < 1e-5, "{forward}");
}

#[test]
fn vehicle_heading_follows_a_bend_and_a_slope_without_rolling() {
    // An L: 20 m along +x, then 20 m along −z. On the second leg the vehicle's +z points −z and
    // its +y stays up (a turn about y). Then a ramp: 20 m along +x rising 5 m. There its +z
    // points along (20, 5, 0) / √425 and its +x stays level (it pitches, never rolls).
    let mut level = Level::new();
    level.lane(&[[0.0, 0.0], [20.0, 0.0], [20.0, -20.0]], 10.0);
    let v = level.vehicle(Vec3::new(18.0, 0.0, 0.0), car());
    let ramp = level.entity(Vec3::new(0.0, 0.0, 100.0));
    let lane = concordia_terra_crowd::Lane {
        points: vec![Vec3::ZERO, Vec3::new(20.0, 5.0, 0.0)],
        speed_limit: 5.0,
        ..concordia_terra_crowd::Lane::default()
    };
    level.world.insert(ramp, lane).unwrap();
    let up = level.vehicle(Vec3::new(2.0, 0.5, 100.0), car());
    let mut rig = Rig::new(level);
    rig.step(240);
    let t = *rig
        .world
        .get::<concordia_scene::builtin::Transform>(v)
        .unwrap();
    assert!(
        t.translation.z < -1.0,
        "on the second leg: {}",
        t.translation
    );
    assert!((t.translation.x - 20.0).abs() < 1e-4);
    assert!((t.rotation * Vec3::Z - Vec3::NEG_Z).length() < 1e-5);
    assert!((t.rotation * Vec3::Y - Vec3::Y).length() < 1e-5, "upright");
    let t = *rig
        .world
        .get::<concordia_scene::builtin::Transform>(up)
        .unwrap();
    let along = Vec3::new(20.0, 5.0, 0.0).normalize();
    assert!(
        (t.rotation * Vec3::Z - along).length() < 1e-5,
        "{}",
        t.rotation * Vec3::Z
    );
    assert!(
        (t.rotation * Vec3::X).y.abs() < 1e-5,
        "level sideways: {}",
        t.rotation * Vec3::X
    );
}

#[test]
fn vehicles_queue_without_overlap() {
    // Five vehicles 20 m apart on a 300 m dead end at 13.9 m/s. The first stops with its front at
    // the end (x = 300: its centre at 297.75); each follower stops behind the one ahead with a
    // bumper gap that the IDM brings down to s₀ = 2 m from above. Every tick, every gap stays
    // positive; after 90 s each is within 0.1 m of 2.
    let mut level = Level::new();
    level.lane(&[[0.0, 0.0], [300.0, 0.0]], 13.9);
    let cars: Vec<EntityId> = (0..5)
        .map(|k| level.vehicle(Vec3::new(90.0 - 20.0 * k as f32, 0.0, 0.0), car()))
        .collect();
    let mut rig = Rig::new(level);
    for tick in 0..5400 {
        rig.step(1);
        for pair in cars.windows(2) {
            let gap = rig.pos(pair[0]).x - rig.pos(pair[1]).x - 4.5;
            assert!(gap > 0.0, "tick {tick}: gap {gap}");
        }
    }
    let front = rig.pos(cars[0]).x + 2.25;
    assert!(
        (front - 300.0).abs() < 0.1,
        "the first stops at the end: {front}"
    );
    for pair in cars.windows(2) {
        let gap = rig.pos(pair[0]).x - rig.pos(pair[1]).x - 4.5;
        assert!((gap - 2.0).abs() < 0.1, "settled gap {gap}");
        assert!(rig.vehicle(pair[1]).speed < 0.05);
    }
}

/// A four-way crossing at the origin: main road west → east (priority 1), side road south →
/// north (priority 0), each an approach lane to x or z = −6, a 12 m connector through the
/// junction, and an exit lane to 100 m.
struct Crossing {
    level: Level,
    main: [EntityId; 3],
    side: [EntityId; 3],
}

fn crossing() -> Crossing {
    let mut level = Level::new();
    let m_in = level.lane(&[[-100.0, 0.0], [-6.0, 0.0]], 10.0);
    let m_x = level.lane(&[[-6.0, 0.0], [6.0, 0.0]], 10.0);
    let m_out = level.lane(&[[6.0, 0.0], [100.0, 0.0]], 10.0);
    let s_in = level.lane(&[[0.0, -100.0], [0.0, -6.0]], 10.0);
    let s_x = level.lane(&[[0.0, -6.0], [0.0, 6.0]], 10.0);
    let s_out = level.lane(&[[0.0, 6.0], [0.0, 100.0]], 10.0);
    level.link(m_in, &[m_x]);
    level.link(m_x, &[m_out]);
    level.link(s_in, &[s_x]);
    level.link(s_x, &[s_out]);
    level
        .world
        .get_mut::<concordia_terra_crowd::Lane>(m_x)
        .unwrap()
        .priority = 1;
    level.junction(&[m_x, s_x]);
    Crossing {
        level,
        main: [m_in, m_x, m_out],
        side: [s_in, s_x, s_out],
    }
}

/// Half a vehicle's footprint: 4.5 m long, 1.8 m wide.
const HALF_LONG: f32 = 2.25;
const HALF_WIDE: f32 = 0.9;

/// Whether a vehicle centred `along` its road reaches into the crossing road's band (its 1.8 m
/// wide strip through the origin).
fn in_band(along: f32) -> bool {
    along + HALF_LONG > -HALF_WIDE && along - HALF_LONG < HALF_WIDE
}

#[test]
fn traffic_follows_lanes_and_yields_at_junctions() {
    // The main road's first vehicle starts with its front 1 m from the line (centre x = −9.25),
    // two more 20 m apart behind it; the side road's first starts with its front 1 m from its
    // line (z = −9.25), one more 20 m behind. All are sent to their exits.
    // - Tick 0: both front vehicles are inside their request distance (v²/(2b) + s₀ = 2 m at
    //   rest), so both ask; the main one's connector has priority 1, so it is granted and the
    //   side one waits (it brakes for its stop line).
    // - The two connectors cross at the origin. Footprints are 4.5 × 1.8 m; a main-road vehicle
    //   and a side-road one overlap exactly when each reaches into the other road's 1.8 m band.
    //   That never happens, and the junction never holds grants for both connectors.
    // - The first side vehicle reaches into the main road's band only after the first main
    //   vehicle's rear has left the side road's band.
    // - Everyone arrives: at their exit's end, or stopped in the queue behind those who did.
    let Crossing {
        mut level,
        main,
        side,
    } = crossing();
    let mains: Vec<EntityId> = (0..3)
        .map(|k| {
            level.vehicle(
                Vec3::new(-9.25 - 20.0 * k as f32, 0.0, 0.0),
                going_to(main[2]),
            )
        })
        .collect();
    let sides: Vec<EntityId> = (0..2)
        .map(|k| {
            level.vehicle(
                Vec3::new(0.0, 0.0, -9.25 - 20.0 * k as f32),
                going_to(side[2]),
            )
        })
        .collect();
    let mut rig = Rig::new(level);
    rig.step(1);
    let granted = |rig: &Rig| -> Vec<(EntityId, EntityId)> {
        rig.world
            .entity_ids_sorted()
            .into_iter()
            .filter_map(|id| rig.world.get::<JunctionState>(id))
            .flat_map(|j| j.granted.iter().map(|g| (g.vehicle, g.lane)))
            .collect()
    };
    assert_eq!(
        granted(&rig),
        vec![(mains[0], main[1])],
        "the main road goes first"
    );
    assert_eq!(rig.vehicle(sides[0]).status, VehicleStatus::Waiting);
    let mut first_main_cleared = None;
    let mut first_side_entered = None;
    for tick in 1..3000u64 {
        rig.step(1);
        let main_in = mains.iter().any(|v| in_band(rig.pos(*v).x));
        let side_in = sides.iter().any(|v| in_band(rig.pos(*v).z));
        assert!(
            !(main_in && side_in),
            "tick {tick}: footprints overlap: {:?} {:?}",
            mains
                .iter()
                .map(|v| (rig.pos(*v), rig.vehicle(*v).status))
                .collect::<Vec<_>>(),
            sides
                .iter()
                .map(|v| (rig.pos(*v), rig.vehicle(*v).status))
                .collect::<Vec<_>>()
        );
        let lanes: Vec<EntityId> = granted(&rig).iter().map(|g| g.1).collect();
        assert!(
            !(lanes.contains(&main[1]) && lanes.contains(&side[1])),
            "tick {tick}: both connectors granted"
        );
        if first_main_cleared.is_none() && rig.pos(mains[0]).x - HALF_LONG >= HALF_WIDE {
            first_main_cleared = Some(tick);
        }
        if first_side_entered.is_none() && in_band(rig.pos(sides[0]).z) {
            first_side_entered = Some(tick);
        }
        let all = mains.iter().chain(&sides);
        if all
            .clone()
            .all(|v| rig.vehicle(*v).status == VehicleStatus::Arrived)
        {
            break;
        }
    }
    let (cleared, entered) = (first_main_cleared.unwrap(), first_side_entered.unwrap());
    assert!(
        entered > cleared,
        "the side waits for the main vehicle: {entered} vs {cleared}"
    );
    for v in mains.iter().chain(&sides) {
        assert_eq!(
            rig.vehicle(*v).status,
            VehicleStatus::Arrived,
            "{v} {:?} at {}",
            rig.vehicle(*v),
            rig.pos(*v)
        );
    }
}

#[test]
fn vehicles_brake_for_people_in_their_lane() {
    // A 200 m lane along +x at 10 m/s. A character (radius 0.3) stands at x = 100, 0.5 m off the
    // centre line (inside 3.5 / 2 + 0.3): the vehicle stops with its front at least s₀ = 2 m
    // short of the character's near edge, x ≤ 100 − 0.3 − 2 (allowing 0.05 m), and never passes.
    // A crowd agent (radius 0.25) at x = 100 does the same. One 2.5 m off the line (past
    // 1.75 + 0.3 = 2.05) isn't in the way: the vehicle drives past it.
    for (offset, blocks) in [(0.5, true), (2.5, false)] {
        let mut level = Level::new();
        level.lane(&[[0.0, 0.0], [200.0, 0.0]], 10.0);
        level.character(Vec3::new(100.0, 0.0, offset), 0.3);
        let v = level.vehicle(Vec3::new(10.0, 0.0, 0.0), car());
        let mut rig = Rig::new(level);
        let mut furthest = 0.0f32;
        for _ in 0..3600 {
            rig.step(1);
            furthest = furthest.max(rig.pos(v).x + 2.25);
        }
        match blocks {
            true => {
                assert!(
                    furthest <= 100.0 - 0.3 - 2.0 + 0.05,
                    "stopped short: {furthest}"
                );
                assert!(furthest > 90.0, "but it drove up: {furthest}");
                assert!(rig.vehicle(v).speed < 0.05);
            }
            false => assert!(furthest > 150.0, "drove past: {furthest}"),
        }
    }
    let mut level = Level::new();
    level.lane(&[[0.0, 0.0], [200.0, 0.0]], 10.0);
    let agent = level.crowd_agent(Vec3::new(100.0, 0.0, 0.0), common::idle());
    let v = level.vehicle(Vec3::new(10.0, 0.0, 0.0), car());
    let mut rig = Rig::new(level);
    rig.step(3600);
    let front = rig.pos(v).x + 2.25;
    assert!(
        front <= 100.0 - 0.25 - 2.0 + 0.05 && front > 90.0,
        "{front}"
    );
    assert!(
        sideways(rig.pos(agent), Vec3::new(100.0, 0.0, 0.0)) < 1e-3,
        "it never moved"
    );
}

#[test]
fn vehicles_reach_their_destination_or_fail_once() {
    // A (0..50 m) leads to B (50..100 m); C (0..50 m at z = 10) and D (0..50 m at z = −10) are
    // connected to nothing. A vehicle on A sent to B stops with its front at B's end (x = 100)
    // and raises one `VehicleArrived { destination: B }`; one on D sent to C (unreachable) fails
    // once with `VehicleRouteFailed { destination: C }` and doesn't move.
    let mut level = Level::new();
    let a = level.lane(&[[0.0, 0.0], [50.0, 0.0]], 10.0);
    let b = level.lane(&[[50.0, 0.0], [100.0, 0.0]], 10.0);
    let c = level.lane(&[[0.0, 10.0], [50.0, 10.0]], 10.0);
    level.lane(&[[0.0, -10.0], [50.0, -10.0]], 10.0);
    level.link(a, &[b]);
    let goes = level.vehicle(Vec3::new(5.0, 0.0, 0.0), going_to(b));
    let lost = level.vehicle(Vec3::new(20.0, 0.0, -10.0), going_to(c));
    let mut rig = Rig::new(level);
    let (mut arrived, mut failed) = (Vec::new(), Vec::new());
    for _ in 0..3600 {
        rig.step(1);
        arrived.extend(
            rig.world
                .pending_signals::<VehicleArrived>()
                .into_iter()
                .map(|(id, s)| (id, *s)),
        );
        failed.extend(
            rig.world
                .pending_signals::<VehicleRouteFailed>()
                .into_iter()
                .map(|(id, s)| (id, *s)),
        );
    }
    assert_eq!(
        arrived.len(),
        1,
        "{arrived:?} {:?} at {}",
        rig.vehicle(goes),
        rig.pos(goes)
    );
    assert_eq!((arrived[0].0, arrived[0].1.destination), (goes, b));
    assert!(
        (arrived[0].1.position.x + 2.25 - 100.0).abs() < 0.5,
        "{:?}",
        arrived[0]
    );
    assert_eq!(rig.vehicle(goes).status, VehicleStatus::Arrived);
    assert_eq!(failed, vec![(lost, VehicleRouteFailed { destination: c })]);
    assert_eq!(rig.vehicle(lost).status, VehicleStatus::Failed);
    assert!(
        (rig.pos(lost).x - 20.0).abs() < 1e-4,
        "it stays: {}",
        rig.pos(lost)
    );
}

#[test]
fn lane_problems_are_descriptive_and_stop_only_their_vehicles() {
    // A good lane and a bad one (one point). The graph names the bad lane's problem, and so does
    // a `TrafficProblems` component on it; a vehicle near the good lane drives; one sent to the
    // bad lane fails.
    let mut level = Level::new();
    let good = level.lane(&[[0.0, 0.0], [100.0, 0.0]], 10.0);
    let bad = level.lane(&[[0.0, 50.0]], 10.0);
    let drives = level.vehicle(Vec3::new(5.0, 0.0, 0.0), car());
    let stuck = level.vehicle(Vec3::new(30.0, 0.0, 0.0), going_to(bad));
    let mut rig = Rig::new(level);
    let graph = rig
        .world
        .resource::<TrafficGraph>()
        .expect("built at play start");
    let problem = graph
        .problems()
        .iter()
        .find(|p| p.entity == bad)
        .expect("named");
    assert!(problem.message.contains("Lane.points"), "{problem:?}");
    assert!(graph.lane(good).is_some() && graph.lane(bad).is_none());
    // The play world also holds each problem on its entity, for `query.run` (designers have no
    // log tool).
    let on_entity = rig
        .world
        .get::<TrafficProblems>(bad)
        .expect("on the bad lane");
    assert_eq!(on_entity.messages, vec![problem.message.clone()]);
    assert!(rig.world.get::<TrafficProblems>(good).is_none());
    rig.step(120);
    assert!(rig.pos(drives).x > 5.5, "{}", rig.pos(drives));
    assert_eq!(rig.vehicle(stuck).status, VehicleStatus::Failed);
}

/// A lane from one junction (by grid cell) to its neighbour.
type Road = ((usize, usize), (usize, usize), EntityId);

/// `n` × `n` junctions 60 m apart, each with four approach lanes and connectors from each to the
/// three other ways out; vehicles fill the lanes between junctions 7 m apart.
fn town(n: usize, vehicles: usize, threads: usize) -> (Rig, Vec<(EntityId, Vec3)>) {
    let mut level = Level::new();
    let at = |i: usize| -((n - 1) as f32) * 30.0 + 60.0 * i as f32;
    // Lanes between neighbours: from junction (i, j) toward (i', j'), on the right-hand side.
    let mut roads: Vec<Road> = Vec::new();
    for i in 0..n {
        for j in 0..n {
            for (di, dj) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (ti, tj) = (i as i32 + di, j as i32 + dj);
                if ti < 0 || tj < 0 || ti >= n as i32 || tj >= n as i32 {
                    continue;
                }
                let (dx, dz) = (di as f32, dj as f32);
                let side = (-dz * 2.0, dx * 2.0);
                let from = (at(i) + dx * 6.0 + side.0, at(j) + dz * 6.0 + side.1);
                let to = (
                    at(ti as usize) - dx * 6.0 + side.0,
                    at(tj as usize) - dz * 6.0 + side.1,
                );
                let id = level.lane(&[[from.0, from.1], [to.0, to.1]], 13.9);
                roads.push(((i, j), (ti as usize, tj as usize), id));
            }
        }
    }
    for i in 0..n {
        for j in 0..n {
            let ins: Vec<_> = roads.iter().filter(|r| r.1 == (i, j)).cloned().collect();
            let outs: Vec<_> = roads.iter().filter(|r| r.0 == (i, j)).cloned().collect();
            let mut connectors = Vec::new();
            for r_in in &ins {
                let mut next = Vec::new();
                for r_out in outs.iter().filter(|o| o.1 != r_in.0) {
                    let end = rig_point(&level, r_in.2, true);
                    let start = rig_point(&level, r_out.2, false);
                    let c = level.lane(&[[end.x, end.z], [start.x, start.z]], 8.0);
                    level.link(c, &[r_out.2]);
                    connectors.push(c);
                    next.push(c);
                }
                level.link(r_in.2, &next);
            }
            if !connectors.is_empty() {
                level.junction(&connectors);
            }
        }
    }
    let mut placed = Vec::new();
    for k in 0..vehicles {
        let road = roads[k % roads.len()].2;
        let slot = (k / roads.len()) as f32;
        let a = rig_point(&level, road, false);
        let b = rig_point(&level, road, true);
        let p = a + (b - a).normalize() * (5.0 + 7.0 * slot);
        placed.push((level.vehicle(p, car()), p));
    }
    (Rig::with(level, threads, |_| {}), placed)
}

/// A lane's first (or last) point.
fn rig_point(level: &Level, lane: EntityId, last: bool) -> Vec3 {
    let l = level
        .world
        .get::<concordia_terra_crowd::Lane>(lane)
        .unwrap();
    *(if last {
        l.points.last()
    } else {
        l.points.first()
    })
    .unwrap()
}

/// The hashes after ticks 1, 30, 60 and 90, and how many vehicles moved over 1 m.
fn run_town(threads: usize) -> (Vec<concordia_terra::WorldHash>, usize) {
    let (mut rig, placed) = town(10, 2000, threads);
    let mut hashes = Vec::new();
    for ticks in [1, 29, 30, 30] {
        rig.step(ticks);
        hashes.push(rig.hash());
    }
    let moved = placed
        .iter()
        .filter(|(id, at)| sideways(rig.pos(*id), *at) > 1.0)
        .count();
    (hashes, moved)
}

#[test]
fn traffic_is_deterministic_across_runs_and_threads() {
    // 2,000 wandering vehicles in a 10 × 10 town (360 lanes between junctions, about 48 m each,
    // up to 6 vehicles a lane): the same world at 1 and 8 threads and on a second run. A vehicle
    // with a free lane ahead covers 0.5 × 1.5² = 1.125 m in 1.5 s from rest, so every vehicle
    // leading its lane has moved over 1 m by tick 90 unless it waits at a junction; followers
    // 7 m behind (a 2.5 m gap) mostly wait. So between 1 and 2,000 moved, and the count matches
    // across runs.
    let (one, moved_one) = run_town(1);
    let (eight, moved_eight) = run_town(8);
    let (again, _) = run_town(8);
    assert_eq!(one, eight, "1 and 8 threads");
    assert_eq!(eight, again, "run to run");
    assert_eq!(moved_one, moved_eight);
    assert!(moved_one > 0 && moved_one < 2000, "{moved_one}");
}
