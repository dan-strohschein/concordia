//! Crowds (2.5c.1) on a rig without physics: surface agents move themselves, so every tick is
//! hand-computable.
//!
//! **Ticks** run at 60 Hz; the first step is tick 0. On each tick `nav.plan` plans, then the crowd
//! steers, integrates, collides and moves, so an agent planned on tick t moves on tick t.
//!
//! **Kinematics** (DetourCrowd's `integrate`): the velocity moves toward the chosen one by at most
//! `max_acceleration × dt` = 8 / 60 = 2/15 m/s a tick, then the agent moves by velocity × dt.
//! From rest toward 3 m/s, the velocity after tick k is min(3, (k + 1) × 2/15): 22 ticks give
//! 2.9333, and tick 22 reaches 3. Ticks 0..=21 travel (2/15) × (1 + … + 22) / 60 = (2/15) × 253 /
//! 60 = 0.562222 m; every later tick travels 0.05 m.
//!
//! **Collisions** (DetourCrowd's loop): 4 Jacobi iterations a tick. For a pair touching nothing
//! else (one contact each), each agent moves (p / 2) × 0.7 apart along the line of centres, so the
//! penetration p becomes 0.3 p per iteration and 0.3⁴ p = 0.0081 p per tick. Against a solid
//! character, the agent alone moves 0.7 p: the same 0.0081 per tick. If a pair closes by at most
//! c a tick (the sum of their speeds × dt), the overlap after a tick is at most 0.0081 × (p + c),
//! which never exceeds 0.0081 c / (1 − 0.0081).

mod common;

use common::{
    Level, Rig, SURFACE, close, deepest_overlap, going, idle, lane, overlap_bound, sideways,
};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_nav::{CrowdAgent, NavAgent, NavFailure, NavStatus};
use concordia_scene::builtin::Transform;
use concordia_terra::{LodProfile, Phase, SystemCtx, World, WorldHash, system};

const EPS: f32 = 1e-3;

#[test]
fn crowd_agent_walks_to_its_target_and_arrives() {
    // From x = −5 to x = 5.23 at 3 m/s on the 10 m floor (walkable to ±9.75): x after tick k ≥ 21
    // is −5 + 0.562222 + 0.05 (k − 21). It arrives on the first tick with at most 0.5 m left:
    // 5.23 − x ≤ 0.5 needs k − 21 ≥ 183.36, so k = 205, at x = 4.762222. The signal is raised then
    // and delivered on tick 206. The first move puts the feet on the surface, y = 0.2.
    let mut level = Level::floor(10.0, 10.0);
    let a = level.crowd_agent(
        Vec3::new(-5.0, 0.0, 0.0),
        going(Vec3::new(5.23, 0.0, 0.0), 3.0),
    );
    let mut rig = Rig::new(level);
    rig.step(1); // tick 0
    let s = rig.crowd(a);
    assert_eq!(rig.nav(a).status, NavStatus::Moving);
    assert!(
        close(s.velocity, Vec3::new(2.0 / 15.0, 0.0, 0.0), 1e-5),
        "{s:?}"
    );
    assert_eq!(s.corners.len(), 1, "one corner, the end: {s:?}");
    assert!(
        close(s.corners[0], Vec3::new(5.23, SURFACE, 0.0), 1e-5),
        "{s:?}"
    );
    assert!(s.at_end);
    let x0 = -5.0 + (2.0 / 15.0) / 60.0;
    assert!(
        close(rig.pos(a), Vec3::new(x0, SURFACE, 0.0), 1e-5),
        "{}",
        rig.pos(a)
    );

    rig.step(21); // ticks 1..=21
    assert!(close(
        rig.crowd(a).velocity,
        Vec3::new(2.9333334, 0.0, 0.0),
        1e-4
    ));
    assert!(
        close(rig.pos(a), Vec3::new(-4.437778, SURFACE, 0.0), EPS),
        "{}",
        rig.pos(a)
    );
    rig.step(1); // tick 22
    assert!(close(rig.crowd(a).velocity, Vec3::new(3.0, 0.0, 0.0), 1e-5));

    rig.step(182); // ticks 23..=204
    assert_eq!(rig.nav(a).status, NavStatus::Moving);
    assert!(rig.arrived().is_empty());
    rig.step(1); // tick 205
    assert_eq!(rig.nav(a).status, NavStatus::Arrived);
    let arrived = rig.arrived();
    assert_eq!(arrived.len(), 1, "{arrived:?}");
    let (who, signal) = arrived[0];
    assert_eq!(who, a);
    assert_eq!(signal.target, Vec3::new(5.23, 0.0, 0.0));
    assert!(
        close(signal.position, Vec3::new(4.762222, SURFACE, 0.0), EPS),
        "{signal:?}"
    );
    assert!(!signal.partial);
    assert_eq!(rig.crowd(a).velocity, Vec3::ZERO);
    rig.step(30);
    assert!(rig.arrived().is_empty(), "raised once");
    assert!(
        close(rig.pos(a), Vec3::new(4.762222, SURFACE, 0.0), EPS),
        "it stays"
    );
}

#[test]
fn crowd_agent_never_steps_past_its_end() {
    // With a 0.02 m arrival distance and 2 m/s² of acceleration, the agent reaches the 2r = 0.5 m
    // slow-down at 3 m/s and needs 3² / (2 × 2) = 2.25 m to stop, so unclamped it would overrun
    // 5.23. The end clamp (2.5b.1's rule) shortens any step longer than what is left: x never
    // passes 5.23, and it arrives within 0.02 m of it, then stays.
    let mut level = Level::floor(10.0, 10.0);
    let exact = NavAgent {
        target_desired_distance: 0.02,
        ..going(Vec3::new(5.23, 0.0, 0.0), 3.0)
    };
    let slow = CrowdAgent {
        max_acceleration: 2.0,
        ..CrowdAgent::default()
    };
    let a = level.crowd_agent_with(Vec3::new(-5.0, 0.0, 0.0), exact, slow);
    let mut rig = Rig::new(level);
    for _ in 0..600 {
        rig.step(1);
        assert!(
            rig.pos(a).x <= 5.23 + 1e-4,
            "never past the end: {}",
            rig.pos(a)
        );
    }
    assert_eq!(rig.nav(a).status, NavStatus::Arrived);
    assert!((rig.pos(a).x - 5.23).abs() <= 0.02, "{}", rig.pos(a));
}

#[test]
fn crowd_agent_slides_along_walls_and_stays_on_the_navmesh() {
    // An idle agent A stands 0.35 m from the floor's edge (walkable to z = 9.75). B walks +z
    // 0.1 m to A's right toward a target past A (arriving only within 0.05 m), so it keeps
    // pushing: A is driven to the edge, where the navmesh holds its centre at z ≤ 9.75
    // (moveAlongSurface clamps every move), on the surface. The push has a −x part (B is to the
    // right), so A slides along the edge in −x until B gets past and arrives. B has avoidance
    // off (it would steer round A instead of pushing it; that is `avoidance.rs`'s subject).
    let mut level = Level::floor(10.0, 10.0);
    let a = level.crowd_agent(Vec3::new(0.0, 0.0, 9.4), idle());
    let b_agent = NavAgent {
        target_desired_distance: 0.05,
        ..going(Vec3::new(0.1, 0.0, 9.65), 3.0)
    };
    let no_avoidance = CrowdAgent {
        avoidance: None,
        ..CrowdAgent::default()
    };
    let b = level.crowd_agent_with(Vec3::new(0.1, 0.0, 7.0), b_agent, no_avoidance);
    let mut rig = Rig::new(level);
    rig.step(1);
    let mut deepest_z = 0.0f32;
    for _ in 0..600 {
        rig.step(1);
        let p = rig.pos(a);
        deepest_z = deepest_z.max(p.z);
        assert!(p.z <= 9.75 + EPS, "held on the navmesh: {p}");
        assert!((p.y - SURFACE).abs() < EPS, "on the surface: {p}");
        if rig.nav(b).status == NavStatus::Arrived {
            break;
        }
    }
    let p = rig.pos(a);
    assert!(deepest_z > 9.7, "pushed to the edge: {deepest_z}");
    assert!(p.x < -0.05, "slid along it, away from B's line: {p}");
    assert_eq!(rig.nav(b).status, NavStatus::Arrived, "B got past");
}

/// The deepest an agent walking from (−9, 0) to (3, −9) on 8 m tiles strays from the straight
/// line, and whether it arrived (straight steering, so corners are walked to exactly; avoidance
/// off, so the floor's walls don't bend the walk).
fn seam_walk(optimize_visibility: bool) -> (f32, Vec<Vec3>, bool) {
    let mut level = Level::floor(10.0, 10.0);
    let volume = level.volume.unwrap();
    level
        .world
        .get_mut::<concordia_nav::NavMeshVolume>(volume)
        .unwrap()
        .tile_size = 32;
    let crowd = CrowdAgent {
        anticipate_turns: false,
        optimize_visibility,
        avoidance: None,
        ..CrowdAgent::default()
    };
    let a = level.crowd_agent_with(
        Vec3::new(-9.0, 0.0, 0.0),
        going(Vec3::new(3.0, 0.0, -9.0), 3.0),
        crowd,
    );
    let mut rig = Rig::new(level);
    rig.step(1);
    let first_corners = rig.crowd(a).corners;
    let line = |p: Vec3| ((p.x + 9.0) * -0.6 - p.z * 0.8).abs();
    let mut deepest = 0.0f32;
    for _ in 0..400 {
        rig.step(1);
        deepest = deepest.max(line(rig.pos(a)));
        if rig.nav(a).status == NavStatus::Arrived {
            break;
        }
    }
    (
        deepest,
        first_corners,
        rig.nav(a).status == NavStatus::Arrived,
    )
}

#[test]
fn crowd_corridor_shortcuts_visible_corners() {
    // Tiles of 32 cells (8 m) from the volume's corner at −12 put seams at x, z = −4 and 4.
    // Detour's A* over portal midpoints routes (−9, 0) → (3, −9) through the tile corner
    // (−4, −4), so the planned path bends there. The corner is 0.2 m off the straight line
    // (direction (0.8, −0.6): |(5, −4) × (0.8, −0.6)| = |−3 + 3.2|), and the straight line lies
    // on open floor. On tick 0 both agents steer by the plan's corners [(−4, −4), the end].
    // - With the shortcut, the ray to the second corner reaches it, so the corridor is cut and
    //   from tick 1 the only corner is the end: the walk stays on the line (its tick-0 step of
    //   0.0022 m toward the corner strays about 7e-5 m).
    // - Without it, straight steering walks to the corner: about 0.2 m off the line.
    let (straight, corners, arrived) = seam_walk(true);
    assert_eq!(corners.len(), 2, "{corners:?}");
    assert!(
        close(corners[0], Vec3::new(-4.0, SURFACE, -4.0), EPS),
        "{corners:?}"
    );
    assert!(arrived);
    assert!(straight < 0.005, "kept to the line: {straight}");
    let (bent, _, arrived) = seam_walk(false);
    assert!(arrived);
    assert!((bent - 0.2).abs() < 0.02, "walked to the corner: {bent}");
}

#[test]
fn crowd_agents_separate_and_do_not_overlap() {
    // 1,000 surface agents (radius 0.25) in 500 lanes: 25 columns 8 m apart by 20 rows 2 m apart.
    // In each lane two agents walk head-on at 3 m/s along x, 6 m apart, offset sideways by 0.1 to
    // 0.4 m (under 2r = 0.5, so they meet). Avoidance and separation are off, so only collision
    // keeps them apart.
    // - Pairs only: a meeting pair separates sideways by at most 2r − offset (each moves at most
    //   0.25 m off its line), so a lane spans at most z − 0.25 .. z + 0.65, and the next lane's
    //   agents stay over 1 m away: every contact is between partners.
    // - Bound: a pair closes by at most 2 × 3 m/s, so (header) no overlap ever exceeds
    //   0.0081 × 0.1 / 0.9919 = 0.000817 m. Float error in positions near x = ±100 is about 1e-5,
    //   so the check allows 0.00085.
    // - Everyone arrives: each pair slides past, then walks to its target.
    let bound = overlap_bound(6.0) + 3.3e-5;
    let mut level = Level::floor(100.0, 22.0);
    let crowd = CrowdAgent {
        avoidance: None,
        ..CrowdAgent::default()
    };
    let mut agents: Vec<EntityId> = Vec::new();
    for k in 0..500 {
        for (start, target) in lane(k) {
            agents.push(level.crowd_agent_with(start, going(target, 3.0), crowd.clone()));
        }
    }
    let mut rig = Rig::new(level);
    let mut deepest = 0.0f32;
    for tick in 0..900 {
        rig.step(1);
        let points: Vec<Vec3> = agents.iter().map(|a| rig.pos(*a)).collect();
        let overlap = deepest_overlap(&points, 0.25);
        deepest = deepest.max(overlap);
        assert!(
            overlap <= bound,
            "tick {tick}: overlap {overlap} over {bound}"
        );
        if agents
            .iter()
            .all(|a| rig.nav(*a).status == NavStatus::Arrived)
        {
            break;
        }
    }
    let waiting: Vec<_> = agents
        .iter()
        .filter(|a| rig.nav(**a).status != NavStatus::Arrived)
        .collect();
    assert!(
        waiting.is_empty(),
        "{} of 1,000 haven't arrived",
        waiting.len()
    );
    assert!(deepest > 0.0, "the pairs did touch");
}

/// Moves `id` along +x at `speed` (a test system in `PrePhysics`, before the crowd).
fn walker(id: EntityId, speed: f32) -> impl FnMut(&mut World, &SystemCtx<'_>) + Send {
    move |world, ctx| {
        if let Some(t) = world.get_mut::<Transform>(id) {
            t.translation.x += speed * ctx.dt;
        }
    }
}

#[test]
fn surface_agents_yield_to_characters_and_never_push_them() {
    // A standing character (radius 0.3) is solid: an agent walking into it takes the whole push.
    // Contact depth 0.55. The agent closes at 3 m/s at most, so (header) its overlap stays under
    // 0.0081 × 0.05 / 0.9919 = 0.000408 m; it slides round (it starts 0.1 m off the line) and
    // arrives, and the character never moves.
    let mut level = Level::floor(10.0, 10.0);
    let still = level.character(Vec3::new(0.0, 0.0, 0.0), 0.3);
    let a = level.crowd_agent(
        Vec3::new(-3.0, 0.0, 0.1),
        going(Vec3::new(3.0, 0.0, 0.1), 3.0),
    );
    let mut rig = Rig::new(level);
    let bound = overlap_bound(3.0) + 1e-5;
    for _ in 0..300 {
        rig.step(1);
        let gap = sideways(rig.pos(a), rig.pos(still));
        assert!(0.55 - gap <= bound, "overlap {} over {bound}", 0.55 - gap);
        assert_eq!(rig.pos(still), Vec3::ZERO, "never pushed");
    }
    assert_eq!(rig.nav(a).status, NavStatus::Arrived);

    // A character walking at 2 m/s through an idle agent 0.15 m off its line shoulders it aside:
    // the agent's overlap stays under 0.0081 × (2 / 60) / 0.9919 = 0.000272 m (measured after the
    // crowd, where the walker has already moved this tick), and the character keeps its line.
    let mut level = Level::floor(10.0, 10.0);
    let moving = level.character(Vec3::new(-3.0, 0.0, 0.0), 0.3);
    let b = level.crowd_agent(Vec3::new(0.0, 0.0, 0.15), idle());
    let mut rig = Rig::with(level, 3, |schedule| {
        schedule.add_in(
            Phase::PrePhysics,
            system("test.walker", walker(moving, 2.0)),
        );
    });
    let bound = overlap_bound(2.0) + 1e-5;
    for tick in 1..=180u32 {
        rig.step(1);
        let gap = sideways(rig.pos(b), rig.pos(moving));
        assert!(0.55 - gap <= bound, "overlap {} over {bound}", 0.55 - gap);
        let x = -3.0 + 2.0 * tick as f32 / 60.0;
        assert!(
            close(rig.pos(moving), Vec3::new(x, 0.0, 0.0), 1e-4),
            "its line"
        );
    }
    assert!(rig.pos(b).z >= 0.55 - 0.001, "pushed aside: {}", rig.pos(b));
}

#[test]
fn crowd_path_failures_are_signalled_once_like_nav_agents() {
    // A crowd agent whose target is 20 m off the floor fails once with `TargetOffNavmesh`, and
    // stands still; a new target that is on the floor then walks.
    let mut level = Level::floor(10.0, 10.0);
    let a = level.crowd_agent(
        Vec3::new(0.0, 0.0, 0.0),
        going(Vec3::new(30.0, 0.0, 0.0), 3.0),
    );
    let mut rig = Rig::new(level);
    rig.step(1);
    let failed = rig.failed();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(
        (failed[0].0, failed[0].1.reason),
        (a, NavFailure::TargetOffNavmesh)
    );
    rig.step(10);
    assert!(rig.failed().is_empty(), "once per target");
    assert_eq!(rig.crowd(a).velocity, Vec3::ZERO);
    assert!(sideways(rig.pos(a), Vec3::ZERO) < 1e-4, "it stands still");
    rig.world.get_mut::<NavAgent>(a).unwrap().target = Some(Vec3::new(3.0, 0.0, 0.0));
    rig.step(60);
    assert_eq!(rig.nav(a).status, NavStatus::Moving);
    assert!(rig.pos(a).x > 0.5, "{}", rig.pos(a));
}

/// 10,000 agents on a 100 × 100 lattice 1.5 m apart, each walking to the point opposite the
/// centre: the hashes after ticks 1, 30, 60 and 90, how many agents moved over 1 m, and which LOD
/// levels were used. `full` turns avoidance on and adds a viewer at one corner with the template
/// example's profile (20, 60, 120 m; rates 2, 4, 8).
fn ten_thousand(threads: usize, full: bool) -> (Vec<WorldHash>, usize, [bool; 4]) {
    let mut level = Level::floor(80.0, 80.0);
    let crowd = match full {
        true => CrowdAgent::default(),
        false => CrowdAgent {
            avoidance: None,
            ..CrowdAgent::default()
        },
    };
    let mut agents = Vec::new();
    for i in 0..100 {
        for j in 0..100 {
            let at = Vec3::new(-74.25 + 1.5 * i as f32, 0.0, -74.25 + 1.5 * j as f32);
            let id =
                level.crowd_agent_with(at, going(Vec3::new(-at.x, 0.0, -at.z), 3.0), crowd.clone());
            agents.push((id, at));
        }
    }
    let mut rig = match full {
        true => {
            let ids: Vec<EntityId> = agents.iter().map(|(id, _)| *id).collect();
            let profile = LodProfile::new([20.0, 60.0, 120.0], [2, 4, 8], 2.0).unwrap();
            level.lod(Vec3::new(-75.0, 0.0, -75.0), &ids, profile);
            Rig::with_lod(level, threads)
        }
        false => Rig::with(level, threads, |_| {}),
    };
    let mut hashes = Vec::new();
    for ticks in [1, 29, 30, 30] {
        rig.step(ticks);
        hashes.push(rig.hash());
    }
    let moved = agents
        .iter()
        .filter(|(id, at)| sideways(rig.pos(*id), *at) > 1.0)
        .count();
    let mut levels = [false; 4];
    for (id, _) in &agents {
        if let Some(l) = rig.lod(*id) {
            levels[usize::from(l.level)] = true;
        }
    }
    (hashes, moved, levels)
}

#[test]
fn crowd_is_deterministic_across_runs_and_threads() {
    // A hash test, not a perf target: the same world at 1 and 8 threads and on a second run.
    // Plain (avoidance off, no LOD; in 2.5c.1 avoidance did nothing, so this is that run): the rig
    // plans 64 agents a tick, longest-waiting then by id. An agent planned on tick t walks on
    // ticks t..=89, and passes 1 m on its 31st tick of walking (0.562222 m over 22 ticks, then
    // 9 × 0.05 m: 1.0122 m; 30 ticks give 0.9622 m). So exactly the agents planned on ticks
    // 0..=59 have moved over 1 m: 60 × 64 = 3,840 (the lowest ids, the lattice's west side,
    // which walk east unhindered for these 1.5 s).
    let (one, moved, _) = ten_thousand(1, false);
    let (eight, _, _) = ten_thousand(8, false);
    let (again, _, _) = ten_thousand(8, false);
    assert_eq!(one, eight, "1 and 8 threads");
    assert_eq!(eight, again, "run to run");
    assert_eq!(moved, 3_840, "the crowd walked as planned");
}

#[test]
fn crowd_with_avoidance_and_lod_is_deterministic_across_runs_and_threads() {
    // The same field with avoidance and LOD on (2.5c.2): the viewer's corner is level 0 and the
    // far corner, 210 m away, level 3, so every level is used. The hashes match across threads
    // and runs, and differ from the plain run's from tick 1 on (agents hold their state at
    // levels 1..=3). Avoidance keeps speeds within the 3 m/s disc (plus 0.001), so no more agents
    // than the plain run's 3,840 can have walked over 1 m.
    let (one, moved, levels) = ten_thousand(1, true);
    let (eight, _, _) = ten_thousand(8, true);
    let (again, _, _) = ten_thousand(8, true);
    assert_eq!(one, eight, "1 and 8 threads, avoidance and LOD on");
    assert_eq!(eight, again, "run to run, avoidance and LOD on");
    assert_eq!(levels, [true; 4], "every level used");
    assert!(
        one.iter()
            .zip(&ten_thousand(1, false).0)
            .all(|(a, b)| a != b),
        "LOD and avoidance change the world"
    );
    assert!(
        moved > 0 && moved <= 3_840,
        "no faster than planning allows: {moved}"
    );
}
