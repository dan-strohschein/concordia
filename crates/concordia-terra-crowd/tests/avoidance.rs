//! Avoidance, walls and simulation LOD (2.5c.2) on the rig without physics (`common`), with
//! `crowd.rs`'s tick, kinematics and collision rules.
//!
//! **Avoidance** (DetourCrowd's sampled velocity obstacles) scores candidate velocities inside
//! the agent's speed disc by how far they stray from the desired and current velocities, which
//! side they pass neighbours on, and how soon they would hit a neighbour or wall within the 2.5 s
//! horizon. Its exact choices are hand-derived in `avoid.rs`'s unit tests; here the outcomes are
//! checked against controls with avoidance off.
//!
//! **Simulation LOD** (ADR-0076): `crowd.corridor`, `crowd.steer` and `crowd.avoid` visit an
//! agent only on its due ticks, so its corners, desired velocity and chosen velocity are held in
//! between. Grid, neighbours, integration, collision and movement run every tick.

mod common;

use common::{Level, Rig, deepest_overlap, going, idle, lane, overlap_bound, sideways};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_nav::{CrowdAgent, NavStatus};
use concordia_terra::LodProfile;
use concordia_terra_crowd::Wall;

fn no_avoidance() -> CrowdAgent {
    CrowdAgent {
        avoidance: None,
        ..CrowdAgent::default()
    }
}

/// Runs two agents head-on for up to 600 ticks: (closest gap, both arrived).
fn head_on(crowd: CrowdAgent) -> (f32, bool) {
    let mut level = Level::floor(10.0, 10.0);
    let a = level.crowd_agent_with(
        Vec3::new(-6.0, 0.0, 0.0),
        going(Vec3::new(6.0, 0.0, 0.0), 3.0),
        crowd.clone(),
    );
    let b = level.crowd_agent_with(
        Vec3::new(6.0, 0.0, 0.1),
        going(Vec3::new(-6.0, 0.0, 0.1), 3.0),
        crowd,
    );
    let mut rig = Rig::new(level);
    let mut closest = f32::MAX;
    for _ in 0..600 {
        rig.step(1);
        closest = closest.min(sideways(rig.pos(a), rig.pos(b)));
        let done = |id| rig.nav(id).status == NavStatus::Arrived;
        if done(a) && done(b) {
            return (closest, true);
        }
    }
    (closest, false)
}

#[test]
fn head_on_agents_pass_without_touching() {
    // Two agents (radius 0.25) walk head-on at 3 m/s, 12 m apart and 0.1 m off each other's line.
    // With avoidance, each sees the other's time of impact inside its horizon from about 15 m of
    // closing (2.5 s × 6 m/s) and picks a velocity that passes it, so they never come within
    // 2r = 0.5 m, and both arrive. With avoidance off, only collision separates them: they touch
    // (2.5c.1's lanes), then slide past and arrive.
    let (closest, arrived) = head_on(CrowdAgent::default());
    assert!(arrived, "both arrive");
    assert!(closest >= 0.5, "never touching: closest {closest}");
    let (touching, arrived) = head_on(no_avoidance());
    assert!(arrived, "both arrive without avoidance too");
    assert!(touching < 0.5, "the control touches: closest {touching}");
}

/// Walks A along a wall and B head-on at it; counts the ticks where A's step left the navmesh
/// (its candidate was clipped back onto the surface).
fn along_the_wall(crowd: CrowdAgent) -> (usize, bool) {
    // The floor is walkable to z = ±0.75.
    let mut level = Level::floor(10.0, 1.0);
    let a = level.crowd_agent_with(
        Vec3::new(-6.0, 0.0, -0.55),
        going(Vec3::new(6.0, 0.0, -0.55), 3.0),
        crowd.clone(),
    );
    let b = level.crowd_agent_with(
        Vec3::new(6.0, 0.0, -0.55),
        going(Vec3::new(-6.0, 0.0, -0.55), 3.0),
        crowd,
    );
    let mut rig = Rig::new(level);
    let mut clipped = 0;
    for _ in 0..600 {
        rig.step(1);
        let s = rig.crowd(a);
        if sideways(s.candidate, s.surface) > 1e-4 {
            clipped += 1;
        }
        let done = |id| rig.nav(id).status == NavStatus::Arrived;
        if done(a) && done(b) {
            return (clipped, true);
        }
    }
    (clipped, false)
}

#[test]
fn avoidance_keeps_agents_off_walls() {
    // A walks +x 0.2 m from the wall at z = −0.75; B walks −x on the same line. Head-on and
    // in line, DetourCrowd's side rule (`prepare`: no relative desired velocity across the line
    // of centres) sends A toward −z, into the wall, and B toward +z. With the wall as an
    // obstacle, A's velocities into it meet it within the horizon (twice the ray time: "avoid less
    // when facing walls"), so A turns away from it instead: its step never leaves the navmesh.
    // The walls are gathered as the local boundary (the nearest 8 within 3.6 m).
    let (clipped, arrived) = along_the_wall(CrowdAgent::default());
    assert!(arrived, "both arrive");
    assert_eq!(clipped, 0, "A never stepped off the navmesh");
}

#[test]
fn walking_agents_gather_their_walls() {
    // A walking agent at (−5, 0.2, 0.25) on the strip walkable to z = ±0.75 has the strip's long
    // edges 0.5 and 1.0 m away and its end 4.75 m away (past the 3.6 m range): its walls are the
    // nearest pieces of z = ±0.75 (at most 8), gathered where it stood, and re-gathered once it
    // has moved over a quarter of the range (0.9 m). An idle agent gathers none.
    let mut level = Level::floor(10.0, 1.0);
    let a = level.crowd_agent(
        Vec3::new(-5.0, 0.0, 0.25),
        going(Vec3::new(5.0, 0.0, 0.25), 3.0),
    );
    let still = level.crowd_agent(Vec3::new(5.0, 0.0, 0.0), idle());
    let mut rig = Rig::new(level);
    rig.step(1);
    let s = rig.crowd(a);
    let center = s.boundary_center.expect("gathered");
    assert!(
        sideways(center, Vec3::new(-5.0, 0.0, 0.25)) < 1e-4,
        "{center}"
    );
    assert!(!s.walls.is_empty() && s.walls.len() <= 8, "{:?}", s.walls);
    for w in &s.walls {
        assert!(
            (w.from.z.abs() - 0.75).abs() < 1e-3 && (w.to.z.abs() - 0.75).abs() < 1e-3,
            "on the long edges: {w:?}"
        );
    }
    // The check reads the grid (where it stood after the tick before). Ticks 0..=21 cover
    // 0.562222 m, then 0.05 m a tick: past 0.9 m after tick 28 (0.912222 m; tick 27 gives
    // 0.862222). So the center stays through tick 28 and moves on tick 29, to x = −4.087778.
    rig.step(28); // ticks 1..=28
    assert_eq!(rig.crowd(a).boundary_center, Some(center), "tick 28");
    rig.step(1); // tick 29
    let moved = rig.crowd(a).boundary_center.unwrap();
    assert!((moved.x - (-5.0 + 0.912222)).abs() < 1e-3, "{moved}");
    assert_eq!(
        rig.crowd(still).boundary_center,
        None,
        "idle agents gather none"
    );
    assert!(rig.crowd(still).walls.is_empty());

    // On a 5.5 m square (walkable to ±2.75), from (−2, 0.2, −2): the near edges x = −2.75 and
    // z = −2.75 are 0.75 m away, and the far ones 4.75 m away (their closest points (2.75, −2)
    // and (−2, 2.75)), past the 3.6 m range, though they bound the same polygons. Only near
    // walls are kept.
    let mut level = Level::floor(3.0, 3.0);
    let a = level.crowd_agent(
        Vec3::new(-2.0, 0.0, -2.0),
        going(Vec3::new(2.0, 0.0, 2.0), 3.0),
    );
    let mut rig = Rig::new(level);
    rig.step(1);
    let walls = rig.crowd(a).walls;
    let near_x = |w: &Wall| (w.from.x + 2.75).abs() < 1e-3 && (w.to.x + 2.75).abs() < 1e-3;
    let near_z = |w: &Wall| (w.from.z + 2.75).abs() < 1e-3 && (w.to.z + 2.75).abs() < 1e-3;
    assert!(walls.iter().any(near_x), "{walls:?}");
    assert!(walls.iter().any(near_z), "{walls:?}");
    assert!(
        walls.iter().all(|w| near_x(w) || near_z(w)),
        "only the near edges: {walls:?}"
    );
}

/// Profile levels 1, 2 and 3 from 20, 60 and 120 m, at rates 2, 4 and 8.
fn profile() -> LodProfile {
    LodProfile::new([20.0, 60.0, 120.0], [2, 4, 8], 2.0).unwrap()
}

/// A walk round a 2 × 4 m box, past an agent standing near the end (so avoidance has a
/// neighbour to weigh every tick), with or without a viewer 200 m away: (arrival tick,
/// ticks on which the desired or chosen velocity changed, ticks that were due).
fn round_the_box(far: bool) -> (Option<u64>, Vec<u64>, Vec<u64>) {
    let mut level = Level::floor(10.0, 10.0);
    level.static_box(Vec3::new(0.0, 0.5, 0.0), Vec3::new(1.0, 0.5, 2.0));
    let a = level.crowd_agent(
        Vec3::new(-4.0, 0.0, 0.0),
        going(Vec3::new(4.0, 0.0, 0.0), 3.0),
    );
    level.crowd_agent(Vec3::new(3.0, 0.0, 0.6), idle());
    if far {
        level.lod(Vec3::new(0.0, 0.0, 200.0), &[a], profile());
    }
    let mut rig = Rig::with_lod(level, 3);
    let (mut changed, mut due) = (Vec::new(), Vec::new());
    let mut last = (Vec3::ZERO, Vec3::ZERO);
    for tick in 0..900u64 {
        rig.step(1);
        let s = rig.crowd(a);
        if (s.desired_velocity, s.new_velocity) != last {
            changed.push(tick);
        }
        last = (s.desired_velocity, s.new_velocity);
        if rig.lod(a).is_some_and(|l| l.due) {
            due.push(tick);
        }
        if far {
            assert_eq!(rig.lod(a).map(|l| l.level), Some(3), "tick {tick}");
        }
        if rig.nav(a).status == NavStatus::Arrived {
            return (Some(tick), changed, due);
        }
    }
    (None, changed, due)
}

#[test]
fn far_crowd_agents_steer_less_often_and_still_arrive() {
    // The box blocks the straight line, so the path bends round two of its corners, and smooth
    // steering changes the desired velocity every tick near them. 200 m from the viewer the agent
    // is level 3, due every 8th tick (its first tick included): its desired velocity and the
    // velocity avoidance chose change only on due ticks, so at most one tick in eight. (Arrival,
    // checked every tick, zeroes them on the arrival tick, due or not.)
    // Arrival: between due ticks it walks on at its held velocity, kept on the navmesh, so each
    // held stretch can carry it at most 7 × 0.05 = 0.35 m past a corner and cost 0.7 m (14
    // ticks) to come back. Over the path's 3 turning points (two of the box's corners, then its
    // end) that is 42 ticks, plus up to 7 for a due tick after planning: within 49 ticks of the
    // full-detail run. (It arrives on tick 197 against 195.)
    let (full, full_changes, _) = round_the_box(false);
    let (lod, changes, due) = round_the_box(true);
    let (full, lod) = (full.expect("arrives"), lod.expect("arrives far"));
    assert!(
        changes.iter().all(|t| due.contains(t) || *t == lod),
        "changes {changes:?} not all on due ticks {due:?}"
    );
    assert!(
        due.len() <= lod as usize / 8 + 2,
        "due every 8th tick: {due:?}"
    );
    assert!(
        full_changes.len() > 3 * changes.len(),
        "full detail steers more: {full_changes:?}"
    );
    assert!(
        lod.abs_diff(full) <= 49,
        "arrived on tick {lod}, full detail on {full}"
    );
}

#[test]
fn neighbours_read_last_tick_state_whatever_their_lod() {
    // A stands idle at the origin (the viewer's spot; it has no profile, so it is level 0). B
    // walks +x at 3 m/s along z = 3.2 from x = −6 without avoidance, 3.2 m from the viewer or
    // more, past the profile's 3 m: level 3, due every 8th tick. Its desired velocity is the same
    // on a straight line, so it walks `crowd.rs`'s kinematics: x after tick k ≥ 21 is
    // −6 + 0.562222 + 0.05 (k − 21).
    // Neighbours are rebuilt every tick from the grid (last tick's positions), B's included,
    // though B steers only when due. B is within A's 3.6 m range (and A within B's) once |x| ≤ √(3.6² − 3.2²) = 1.649242: from tick 97 (x = −1.637778), so A
    // lists it from tick 98; it leaves after tick 163 (x = 1.662222), so A drops it on tick 164.
    let mut level = Level::floor(10.0, 10.0);
    let a = level.crowd_agent(Vec3::ZERO, idle());
    let b = level.crowd_agent_with(
        Vec3::new(-6.0, 0.0, 3.2),
        going(Vec3::new(6.0, 0.0, 3.2), 3.0),
        no_avoidance(),
    );
    let near = LodProfile::new([1.0, 2.0, 3.0], [2, 4, 8], 0.1).unwrap();
    level.lod(Vec3::ZERO, &[b], near);
    let mut rig = Rig::with_lod(level, 3);
    for tick in 0..=200u64 {
        rig.step(1);
        assert_eq!(rig.lod(b).map(|l| l.level), Some(3), "tick {tick}");
        let listed = rig.crowd(a).neighbours.contains(&b);
        assert_eq!(listed, (98..=163).contains(&tick), "tick {tick}");
        let back = rig.crowd(b).neighbours.contains(&a);
        assert_eq!(
            back, listed,
            "B's neighbours are rebuilt every tick too: tick {tick}"
        );
        if tick == 97 {
            assert!((rig.pos(b).x - -1.637778).abs() < 1e-3, "{}", rig.pos(b));
        }
    }
}

#[test]
fn crowd_agents_avoid_each_other_at_a_thousand_with_lod() {
    // 2.5c.1's 1,000 agents in 500 lanes (`common::lane`), now with avoidance on and a viewer at
    // the field's centre: agents past 20 and 60 m are levels 1 and 2 (the field reaches about
    // 100 m, short of level 3's 120 m).
    // - Avoidance only picks velocities inside the 3 m/s disc (plus 0.001), so a pair still closes
    //   by at most 2 × 3.001 m/s, and collision's bound stays 0.0081 × 0.100033 / 0.9919.
    // - That bound is for contacts between partners. Lanes are 2 m apart and partners at most
    //   0.4 m apart sideways, so as long as no agent strays 0.55 m from its own line, agents of
    //   different lanes stay over 0.5 m apart. The test checks that every tick.
    // - Everyone arrives.
    let bound = overlap_bound(6.002) + 3.3e-5;
    let mut level = Level::floor(100.0, 22.0);
    let mut agents: Vec<(EntityId, f32)> = Vec::new();
    for k in 0..500 {
        for (start, target) in lane(k) {
            let id = level.crowd_agent(start, going(target, 3.0));
            agents.push((id, start.z));
        }
    }
    let ids: Vec<EntityId> = agents.iter().map(|(id, _)| *id).collect();
    level.lod(Vec3::ZERO, &ids, profile());
    let mut rig = Rig::with_lod(level, 3);
    let mut levels = [false; 4];
    for tick in 0..900 {
        rig.step(1);
        let points: Vec<Vec3> = ids.iter().map(|a| rig.pos(*a)).collect();
        let overlap = deepest_overlap(&points, 0.25);
        assert!(
            overlap <= bound,
            "tick {tick}: overlap {overlap} over {bound}"
        );
        for ((id, line), p) in agents.iter().zip(&points) {
            assert!(
                (p.z - line).abs() < 0.55,
                "tick {tick}: {id} strayed to {p}"
            );
            if let Some(l) = rig.lod(*id) {
                levels[usize::from(l.level)] = true;
            }
        }
        if ids.iter().all(|a| rig.nav(*a).status == NavStatus::Arrived) {
            break;
        }
    }
    let waiting = ids
        .iter()
        .filter(|a| rig.nav(**a).status != NavStatus::Arrived)
        .count();
    assert_eq!(waiting, 0, "{waiting} of 1,000 haven't arrived");
    assert_eq!(
        levels,
        [true, true, true, false],
        "levels 0, 1 and 2 were all used"
    );
}
