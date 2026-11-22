//! Navigation agents (2.5b.1): planning, following, arrival, replanning, failures, the plan
//! budget and LOD, on a rig without physics, so every tick is hand-computable.
//!
//! **The rig.** A test system in `Physics` moves each agent's `Transform` by
//! `CharacterMotion.velocity × dt` (y ignored, as the character controller does). Ticks run at
//! 60 Hz, so an agent at 3 m/s moves 0.05 m a tick. The first step is tick 0; `nav.follow` on tick
//! t sees the position after t moves.
//!
//! **The floor** (2.5a's `query.rs`): a slab of half 10.1 under a 24 m volume of one 128-cell
//! tile, with 0.25 m cells and a 0.25 m agent radius. It is walkable to ±9.75 at y = 0.2 (a span
//! is at least one cell tall), so feet at y = 0 are 0.2 m under the surface, inside half the agent
//! height (0.9 m). Distances are sideways (x and z).
//!
//! **Arrival.** An agent going from x = −5 to x = 5.23 at 3 m/s has 10.23 − 0.05 t left on tick t.
//! It arrives on the first tick with at most 0.5 left: 10.23 − 0.05 t ≤ 0.5, so t ≥ 194.6 and
//! t = 195, at x = −5 + 0.05 × 195 = 4.75. The signal is raised then and delivered on tick 196.
//! With a 0.02 m arrival distance, 0.03 m is left on tick 204 (10.23 − 10.2), so the last step is
//! slowed to 0.03 / dt and lands on 5.23; it arrives on tick 205.

mod common;

use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::{Arc, Mutex};

use common::{Level, SURFACE, close};
use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;
use concordia_core::math::{Quat, Vec3};
use concordia_core::time::TickRate;
use concordia_nav::{
    AgentConfig, CrowdAgent, CrowdAvoidance, NavAgent, NavAgentState, NavArrived, NavError,
    NavFailure, NavMeshes, NavPathFailed, NavStatus, add_agent_processors, insert_agent_states,
};
use concordia_physics::{CharacterController, CharacterMotion};
use concordia_scene::builtin::{Transform, lod_position};
use concordia_terra::{
    LodProfile, LodViewer, Phase, Schedule, SimLoop, SystemCtx, View, World, lod_system, processor,
    system,
};

const EPS: f32 = 1e-3;

struct Rig {
    world: World,
    sim: SimLoop,
    jobs: JobSystem,
    ticks: u64,
}

impl Rig {
    fn new(level: Level, plans: u32) -> Self {
        Self::with(level, plans, |_| {})
    }

    /// The navmesh of `level`'s volume, the agent processors, the mover and `extra`.
    fn with(mut level: Level, plans: u32, extra: impl FnOnce(&mut Schedule)) -> Self {
        let mesh = Arc::new(level.build());
        let volume = level.volume.expect("a volume");
        level
            .world
            .insert_resource(NavMeshes::new([(volume, mesh)]));
        insert_agent_states(&mut level.world);
        let mut schedule = Schedule::new();
        extra(&mut schedule);
        add_agent_processors(
            &mut schedule,
            AgentConfig {
                plans_per_tick: NonZeroU32::new(plans).unwrap(),
            },
        );
        schedule.add_in(Phase::Physics, system("test.mover", mover));
        let mut world = level.world;
        let sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
        Self {
            world,
            sim,
            jobs: JobSystem::new(NonZeroUsize::new(3).unwrap()).unwrap(),
            ticks: 0,
        }
    }

    fn step(&mut self, ticks: u32) {
        self.sim.step(&mut self.world, &self.jobs, ticks);
        self.ticks += u64::from(ticks);
    }

    fn pos(&self, id: EntityId) -> Vec3 {
        self.world.get::<Transform>(id).unwrap().translation
    }

    fn state(&self, id: EntityId) -> NavAgentState {
        self.world.get::<NavAgentState>(id).unwrap().clone()
    }

    fn agent(&mut self, id: EntityId) -> &mut NavAgent {
        self.world.get_mut::<NavAgent>(id).unwrap()
    }

    fn teleport(&mut self, id: EntityId, to: Vec3) {
        self.world.get_mut::<Transform>(id).unwrap().translation = to;
    }

    fn arrived(&self) -> Vec<(EntityId, NavArrived)> {
        self.world
            .pending_signals::<NavArrived>()
            .into_iter()
            .map(|(id, s)| (id, *s))
            .collect()
    }

    fn failed(&self) -> Vec<(EntityId, NavPathFailed)> {
        self.world
            .pending_signals::<NavPathFailed>()
            .into_iter()
            .map(|(id, s)| (id, s.clone()))
            .collect()
    }
}

/// Moves each character by its desired horizontal velocity, as the controller would on flat
/// ground.
fn mover(world: &mut World, ctx: &SystemCtx<'_>) {
    let Ok(mut query) = world.query::<(&mut Transform, &CharacterMotion)>() else {
        return;
    };
    query.for_each(world, |_, (t, m)| {
        t.translation += Vec3::new(m.velocity.x, 0.0, m.velocity.z) * ctx.dt;
    });
}

/// The 2.5a floor: one tile, walkable to ±9.75 at y = 0.2.
fn floor() -> Level {
    let mut level = Level::new();
    level.slab(0.0, 0.0, 10.1, 10.1, 0.0);
    let mut settings = common::volume_settings(Vec3::new(12.0, 3.0, 12.0));
    settings.tile_size = 128;
    level.volume(Vec3::ZERO, settings);
    level
}

/// An agent (sized like the fixture volume's agent) with a matching controller.
fn agent(level: &mut Level, at: Vec3, nav: NavAgent) -> EntityId {
    let id = level.entity(at, Quat::IDENTITY);
    level.world.insert(id, nav).unwrap();
    let controller = CharacterController {
        radius: 0.25,
        ..CharacterController::default()
    };
    level.world.insert(id, controller).unwrap();
    level.world.insert(id, CharacterMotion::default()).unwrap();
    id
}

fn going(to: Vec3, speed: f32) -> NavAgent {
    NavAgent {
        target: Some(to),
        max_speed: speed,
        radius: 0.25,
        ..NavAgent::default()
    }
}

fn on(x: f32, z: f32) -> Vec3 {
    Vec3::new(x, SURFACE, z)
}

fn sideways(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(a.x - b.x, 0.0, a.z - b.z).length()
}

#[test]
fn nav_agent_walks_a_straight_path_and_arrives_within_tolerance() {
    let mut level = floor();
    let a = agent(
        &mut level,
        Vec3::new(-5.0, 0.0, 0.0),
        going(Vec3::new(5.23, 0.0, 0.0), 3.0),
    );
    let exact = NavAgent {
        target_desired_distance: 0.02,
        ..going(Vec3::new(5.23, 0.0, 2.0), 3.0)
    };
    let b = agent(&mut level, Vec3::new(-5.0, 0.0, 2.0), exact);
    let mut rig = Rig::new(level, 32);
    assert_eq!(
        rig.state(a).status,
        NavStatus::Idle,
        "a state from play start"
    );

    rig.step(1);
    let s = rig.state(a);
    assert_eq!(s.status, NavStatus::Moving, "{s:?}");
    assert_eq!(s.planned_for, Some(Vec3::new(5.23, 0.0, 0.0)));
    assert_eq!(s.waypoints.len(), 2, "{s:?}");
    assert!(close(s.waypoints[0], on(-5.0, 0.0), EPS), "{s:?}");
    assert!(close(s.waypoints[1], on(5.23, 0.0), EPS), "{s:?}");
    assert_eq!((s.next, s.partial), (1, false));
    let v = rig.world.get::<CharacterMotion>(a).unwrap().velocity;
    assert!(close(v, Vec3::new(3.0, 0.0, 0.0), 1e-5), "{v}");
    assert!(close(rig.pos(a), Vec3::new(-4.95, 0.0, 0.0), 1e-5));

    rig.step(194); // ticks 1..=194
    assert_eq!(rig.state(a).status, NavStatus::Moving);
    assert!(rig.arrived().is_empty());
    rig.step(1); // tick 195
    assert_eq!(rig.state(a).status, NavStatus::Arrived);
    let arrived = rig.arrived();
    assert_eq!(arrived.len(), 1, "{arrived:?}");
    let (who, signal) = arrived[0];
    assert_eq!(who, a);
    assert_eq!(signal.target, Vec3::new(5.23, 0.0, 0.0));
    assert!(
        close(signal.position, Vec3::new(4.75, 0.0, 0.0), EPS),
        "{signal:?}"
    );
    assert!(!signal.partial);
    assert_eq!(
        rig.world.get::<CharacterMotion>(a).unwrap().velocity,
        Vec3::ZERO
    );
    rig.step(1); // tick 196: delivered, not raised again
    assert!(rig.arrived().iter().all(|(id, _)| *id != a));
    assert!(
        close(rig.pos(a), Vec3::new(4.75, 0.0, 0.0), EPS),
        "it stays"
    );

    rig.step(8); // ticks 197..=204
    assert_eq!(rig.state(b).status, NavStatus::Moving);
    assert!(
        close(rig.pos(b), Vec3::new(5.23, 0.0, 2.0), EPS),
        "slowed onto the end: {}",
        rig.pos(b)
    );
    rig.step(1); // tick 205
    assert_eq!(rig.state(b).status, NavStatus::Arrived);
    assert_eq!(rig.arrived().len(), 1);
    rig.step(30);
    assert!(
        close(rig.pos(b), Vec3::new(5.23, 0.0, 2.0), EPS),
        "no overshoot"
    );
}

/// The distance from `p` to the polyline `path`, sideways.
fn off_path(p: Vec3, path: &[Vec3]) -> f32 {
    path.windows(2)
        .map(|w| {
            let (a, b) = (
                Vec3::new(w[0].x, 0.0, w[0].z),
                Vec3::new(w[1].x, 0.0, w[1].z),
            );
            let q = Vec3::new(p.x, 0.0, p.z);
            let t = ((q - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            q.distance(a + (b - a) * t)
        })
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn nav_agent_turns_corners_within_path_desired_distance() {
    // 2.5a's wall (`path_avoids_blocked_cells`): blocked x ∈ [−1, 1), z ∈ [−10, 6), so the path
    // from (−5, 0) to (5, 0) has corners (−1.25, 6), (−1, 6.25), (1, 6.25) and (1.25, 6).
    //
    // An agent heads straight at its next corner and turns once within r = 0.2 of it. Each
    // stretch from where it turned (p, |p − cᵢ| ≤ r) to the next corner cᵢ₊₁ stays within r of the
    // path segment cᵢ → cᵢ₊₁: the point a fraction t along p → cᵢ₊₁ is (1 − t)|p − cᵢ| ≤ r from the
    // point t along cᵢ → cᵢ₊₁. The path keeps the agent radius (0.25) from the wall, so with
    // r = 0.2 it never enters the wall.
    let mut level = floor();
    let mut rows = vec![".".repeat(20); 20];
    for row in &mut rows[..16] {
        row.replace_range(9..11, "##");
    }
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    level.grid(-10.0, -10.0, 1.0, &rows);
    let nav = NavAgent {
        path_desired_distance: 0.2,
        ..going(Vec3::new(5.0, 0.0, 0.0), 3.0)
    };
    let a = agent(&mut level, Vec3::new(-5.0, 0.0, 0.0), nav);
    let mut rig = Rig::new(level, 32);
    rig.step(1);
    let path = rig.state(a).waypoints;
    let expected = [
        on(-5.0, 0.0),
        on(-1.25, 6.0),
        on(-1.0, 6.25),
        on(1.0, 6.25),
        on(1.25, 6.0),
        on(5.0, 0.0),
    ];
    assert_eq!(path.len(), expected.len(), "{path:?}");
    let mut corners = vec![1];
    for _ in 0..600 {
        let p = rig.pos(a);
        assert!(
            off_path(p, &path) <= 0.2 + 1e-4,
            "{p} is {} off the path",
            off_path(p, &path)
        );
        assert!(
            !(p.x > -1.0 && p.x < 1.0 && p.z < 6.0),
            "{p} is in the wall"
        );
        let s = rig.state(a);
        if corners.last() != Some(&s.next) {
            corners.push(s.next);
        }
        if s.status == NavStatus::Arrived {
            break;
        }
        rig.step(1);
    }
    assert_eq!(rig.state(a).status, NavStatus::Arrived);
    assert_eq!(corners, [1, 2, 3, 4, 5], "every corner in turn");
    assert!(sideways(rig.pos(a), on(5.0, 0.0)) <= 0.5);
}

#[test]
fn nav_agent_replans_when_the_target_moves_past_its_tolerance() {
    let mut level = floor();
    let a = agent(
        &mut level,
        Vec3::new(-5.0, 0.0, 0.0),
        going(Vec3::new(5.0, 0.0, 0.0), 3.0),
    );
    let mut rig = Rig::new(level, 32);
    rig.step(1);
    let first = rig.state(a);
    // 0.4 m sideways is inside the 0.5 m arrival distance: the same path.
    rig.agent(a).target = Some(Vec3::new(5.0, 0.0, 0.4));
    rig.step(1);
    assert_eq!(rig.state(a), first, "a small move keeps the plan");
    // 0.6 m is past it: plan again from where the agent is (x = −5 + 2 × 0.05).
    rig.agent(a).target = Some(Vec3::new(5.0, 0.0, 0.6));
    rig.step(1);
    let s = rig.state(a);
    assert_eq!(s.planned_for, Some(Vec3::new(5.0, 0.0, 0.6)));
    assert!(close(s.waypoints[0], on(-4.9, 0.0), EPS), "{s:?}");
    assert!(
        close(*s.waypoints.last().unwrap(), on(5.0, 0.6), EPS),
        "{s:?}"
    );
    // 1.5 m straight up is past half the agent height (0.9 m): a different target (it snaps
    // to the same floor, within the navmesh's 2 m vertical reach).
    rig.agent(a).target = Some(Vec3::new(5.0, 1.5, 0.6));
    rig.step(1);
    let s = rig.state(a);
    assert_eq!(s.planned_for, Some(Vec3::new(5.0, 1.5, 0.6)), "{s:?}");
    assert!(
        close(*s.waypoints.last().unwrap(), on(5.0, 0.6), EPS),
        "{s:?}"
    );
    // Clearing the target stops the agent.
    rig.agent(a).target = None;
    rig.step(1);
    assert_eq!(rig.state(a).status, NavStatus::Idle);
    assert_eq!(
        rig.world.get::<CharacterMotion>(a).unwrap().velocity,
        Vec3::ZERO
    );
}

#[test]
fn nav_agent_replans_when_pushed_off_its_path() {
    // The path is (−5, 0) → (5, 0); pushed to z = 1.5 the agent is 1.5 m off it (under the 2 m
    // path_max_distance) and walks back toward the corner; at z = 2.5 it plans again from there.
    let mut level = floor();
    let a = agent(
        &mut level,
        Vec3::new(-5.0, 0.0, 0.0),
        going(Vec3::new(5.0, 0.0, 0.0), 3.0),
    );
    let mut rig = Rig::new(level, 32);
    rig.step(1);
    rig.teleport(a, Vec3::new(-3.0, 0.0, 1.5));
    rig.step(1);
    let s = rig.state(a);
    assert!(close(s.waypoints[0], on(-5.0, 0.0), EPS), "kept: {s:?}");
    rig.teleport(a, Vec3::new(-3.0, 0.0, 2.5));
    rig.step(1);
    let s = rig.state(a);
    assert!(
        close(s.waypoints[0], on(-3.0, 2.5), EPS),
        "replanned: {s:?}"
    );
    assert_eq!(s.planned_for, Some(Vec3::new(5.0, 0.0, 0.0)));
}

#[test]
fn unreachable_target_walks_to_the_closest_point_and_arrives_partial() {
    // 2.5a's two floors with a gap: the left one is walkable to x = −1.25. From x = −6.03 the
    // closest point is 4.78 m away; 4.78 − 0.05 t ≤ 0.5 first holds at t = 86 (x = −1.73).
    let mut level = Level::new();
    level.slab(-5.5, 0.0, 4.6, 5.1, 0.0);
    level.slab(5.5, 0.0, 4.6, 5.1, 0.0);
    level.standard_volume(12.0);
    let a = agent(
        &mut level,
        Vec3::new(-6.03, 0.0, 0.0),
        going(Vec3::new(6.0, 0.0, 0.0), 3.0),
    );
    let mut rig = Rig::new(level, 32);
    rig.step(1);
    let s = rig.state(a);
    assert!(s.partial, "{s:?}");
    assert!(
        close(*s.waypoints.last().unwrap(), on(-1.25, 0.0), EPS),
        "{s:?}"
    );
    rig.step(85); // ticks 1..=85
    assert!(rig.arrived().is_empty());
    rig.step(1); // tick 86
    let arrived = rig.arrived();
    assert_eq!(arrived.len(), 1, "{arrived:?}");
    assert!(arrived[0].1.partial);
    assert_eq!(arrived[0].1.target, Vec3::new(6.0, 0.0, 0.0));
    assert!(
        close(arrived[0].1.position, Vec3::new(-1.73, 0.0, 0.0), EPS),
        "{arrived:?}"
    );
    assert!(rig.failed().is_empty(), "unreachable isn't a failure");
}

#[test]
fn nav_path_failed_names_the_reason_once_per_target() {
    // The nearest-point reach is 2 m sideways, but Detour tests polygons by quantized bounds, so
    // a point just past it can still snap. The volume here is 16 m across: (14.5, 0) is inside it
    // and 4.75 m from the walkable edge (x = 9.75), well off the navmesh.
    let mut level = Level::new();
    level.slab(0.0, 0.0, 10.1, 10.1, 0.0);
    let mut settings = common::volume_settings(Vec3::new(16.0, 3.0, 16.0));
    settings.tile_size = 128;
    level.volume(Vec3::ZERO, settings);
    let outside = agent(
        &mut level,
        Vec3::new(40.0, 0.0, 0.0),
        going(Vec3::ZERO, 3.0),
    );
    let start_off = agent(
        &mut level,
        Vec3::new(14.5, 0.0, 0.0),
        going(Vec3::ZERO, 3.0),
    );
    let target_off = agent(
        &mut level,
        Vec3::ZERO,
        going(Vec3::new(40.0, 0.0, 0.0), 3.0),
    );
    let slow = agent(&mut level, Vec3::new(1.0, 0.0, 1.0), going(Vec3::ZERO, 0.0));
    let bare = level.entity(Vec3::new(2.0, 0.0, 2.0), Quat::IDENTITY);
    level.world.insert(bare, going(Vec3::ZERO, 3.0)).unwrap();
    let mut rig = Rig::new(level, 32);
    rig.step(1);
    let failed = rig.failed();
    let reason = |id: EntityId| {
        let found: Vec<_> = failed.iter().filter(|(who, _)| *who == id).collect();
        assert_eq!(found.len(), 1, "{id}: {failed:?}");
        found[0].1.clone()
    };
    assert_eq!(reason(outside).reason, NavFailure::NoNavmesh);
    assert_eq!(reason(start_off).reason, NavFailure::StartOffNavmesh);
    assert_eq!(reason(target_off).reason, NavFailure::TargetOffNavmesh);
    assert_eq!(reason(target_off).target, Vec3::new(40.0, 0.0, 0.0));
    assert_eq!(reason(slow).reason, NavFailure::InvalidAgent);
    assert!(reason(slow).detail.contains("max_speed"), "{failed:?}");
    assert_eq!(reason(bare).reason, NavFailure::InvalidAgent);
    assert!(
        reason(bare).detail.contains("CharacterController"),
        "{failed:?}"
    );
    for id in [outside, start_off, target_off, slow, bare] {
        assert_eq!(rig.state(id).status, NavStatus::Failed, "{id}");
    }
    assert_eq!(
        rig.world
            .get::<CharacterMotion>(target_off)
            .unwrap()
            .velocity,
        Vec3::ZERO
    );
    rig.step(2);
    assert!(rig.failed().is_empty(), "once per target");
    rig.agent(target_off).target = Some(Vec3::new(41.0, 0.0, 0.0));
    rig.step(1);
    assert_eq!(rig.failed().len(), 1, "a new target is tried again");
}

/// Spawns floor agents with these ids (in reverse, so spawn order isn't id order).
fn agents_with_ids(level: &mut Level, ids: &[&str]) -> Vec<EntityId> {
    let ids: Vec<EntityId> = ids.iter().map(|s| s.parse().unwrap()).collect();
    for (i, id) in ids.iter().enumerate().rev() {
        level.world.spawn_with_id(*id).unwrap();
        let z = i as f32 - 2.0;
        let t = Transform {
            translation: Vec3::new(-5.0, 0.0, z),
            ..Transform::default()
        };
        level.world.insert(*id, t).unwrap();
        level
            .world
            .insert(*id, going(Vec3::new(5.0, 0.0, z), 3.0))
            .unwrap();
        let controller = CharacterController {
            radius: 0.25,
            ..CharacterController::default()
        };
        level.world.insert(*id, controller).unwrap();
        level.world.insert(*id, CharacterMotion::default()).unwrap();
    }
    ids
}

const IDS: [&str; 5] = [
    "018bcfe5-6800-7000-a424-0000000000a0",
    "018bcfe5-6800-7000-a424-0000000000b0",
    "018bcfe5-6800-7000-a424-0000000000c0",
    "018bcfe5-6800-7000-a424-0000000000d0",
    "018bcfe5-6800-7000-a424-0000000000e0",
];

#[test]
fn plans_are_budgeted_per_tick_longest_waiting_first() {
    // Two plans a tick for five agents: ids a, b on tick 0, c, d on tick 1, e on tick 2.
    let mut level = floor();
    let ids = agents_with_ids(&mut level, &IDS);
    let mut rig = Rig::new(level, 2);
    let statuses = |rig: &Rig, ids: &[EntityId]| -> Vec<NavStatus> {
        ids.iter().map(|id| rig.state(*id).status).collect()
    };
    use NavStatus::{Moving, Waiting};
    rig.step(1);
    assert_eq!(
        statuses(&rig, &ids),
        [Moving, Moving, Waiting, Waiting, Waiting]
    );
    assert_eq!(rig.state(ids[2]).waiting_since, 0);
    rig.step(1);
    assert_eq!(
        statuses(&rig, &ids),
        [Moving, Moving, Moving, Moving, Waiting]
    );
    rig.step(1);
    assert_eq!(statuses(&rig, &ids), [Moving; 5]);

    // One plan a tick for a < b < c: a plans on tick 0; b and c wait from tick 0. a's target then
    // moves, so it waits from tick 1: b (waiting since 0) goes on tick 1, c on tick 2, a on 3.
    let mut level = floor();
    let ids = agents_with_ids(&mut level, &IDS[..3]);
    let mut rig = Rig::new(level, 1);
    rig.step(1);
    assert_eq!(statuses(&rig, &ids), [Moving, Waiting, Waiting]);
    rig.agent(ids[0]).target = Some(Vec3::new(5.0, 0.0, 3.0));
    rig.step(1);
    assert_eq!(statuses(&rig, &ids), [Waiting, Moving, Waiting]);
    assert_eq!(rig.state(ids[0]).waiting_since, 1);
    let v = rig.world.get::<CharacterMotion>(ids[0]).unwrap().velocity;
    assert!(
        v.length() > 2.9,
        "a keeps walking its old path while it waits: {v}"
    );
    rig.step(1);
    assert_eq!(statuses(&rig, &ids), [Waiting, Moving, Moving]);
    rig.step(1);
    assert_eq!(statuses(&rig, &ids), [Moving, Moving, Moving]);
    assert_eq!(
        rig.state(ids[0]).planned_for,
        Some(Vec3::new(5.0, 0.0, 3.0))
    );
}

type Heard = Arc<Mutex<Vec<(u64, EntityId)>>>;

#[test]
fn far_agents_follow_at_full_rate_and_signals_ignore_lod() {
    // Both agents make the straight walk of the header and arrive on tick 195. The second is level
    // 3 (16 m from the viewer, past 3 m) with a rate of 8, but the agent processors don't opt into
    // LOD, so it follows every tick; an LOD-aware receiver still hears both arrivals.
    let mut level = floor();
    let near = agent(
        &mut level,
        Vec3::new(-5.0, 0.0, -9.0),
        going(Vec3::new(5.23, 0.0, -9.0), 3.0),
    );
    let far = agent(
        &mut level,
        Vec3::new(-5.0, 0.0, 7.0),
        going(Vec3::new(5.23, 0.0, 7.0), 3.0),
    );
    let viewer = level.entity(Vec3::new(0.0, 0.0, -9.0), Quat::IDENTITY);
    level.world.insert(viewer, LodViewer).unwrap();
    level.world.register_shared::<LodProfile>().unwrap();
    let profile = LodProfile::new([1.0, 2.0, 3.0], [2, 4, 8], 0.5).unwrap();
    level.world.insert(far, profile).unwrap();
    let heard: Heard = Arc::default();
    let sink = heard.clone();
    let mut rig = Rig::with(level, 32, move |schedule| {
        schedule.add_in(Phase::PrePhysics, lod_system(lod_position));
        schedule.add_processor(
            processor("test.hear", Phase::PostPhysics)
                .receives::<NavArrived>()
                .reads::<NavAgent>()
                .lod()
                .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| {
                    for (id, _) in view.signals::<NavArrived>().unwrap() {
                        sink.lock().unwrap().push((ctx.tick.0, *id));
                    }
                }),
        );
    });
    rig.step(197);
    assert_eq!(
        rig.world
            .get::<concordia_terra::SimLod>(far)
            .map(|l| l.level),
        Some(3)
    );
    assert!(
        close(rig.pos(far), Vec3::new(4.75, 0.0, 7.0), EPS),
        "{}",
        rig.pos(far)
    );
    assert!(close(rig.pos(near), Vec3::new(4.75, 0.0, -9.0), EPS));
    let mut got = heard.lock().unwrap().clone();
    got.sort();
    let mut want = vec![(196, near), (196, far)];
    want.sort();
    assert_eq!(got, want, "both delivered on tick 196");
}

#[test]
fn malformed_nav_agents_are_descriptive_errors() {
    let id: EntityId = IDS[0].parse().unwrap();
    assert!(NavAgent::default().validate(id).is_ok());
    let cases: Vec<(&str, NavAgent)> = vec![
        (
            "max_speed",
            NavAgent {
                max_speed: 0.0,
                ..NavAgent::default()
            },
        ),
        (
            "max_speed",
            NavAgent {
                max_speed: f32::NAN,
                ..NavAgent::default()
            },
        ),
        (
            "radius",
            NavAgent {
                radius: 0.0,
                ..NavAgent::default()
            },
        ),
        (
            "height",
            NavAgent {
                height: f32::INFINITY,
                ..NavAgent::default()
            },
        ),
        (
            "path_desired_distance",
            NavAgent {
                path_desired_distance: -1.0,
                ..NavAgent::default()
            },
        ),
        (
            "target_desired_distance",
            NavAgent {
                target_desired_distance: 0.0,
                ..NavAgent::default()
            },
        ),
        (
            "path_max_distance",
            NavAgent {
                path_max_distance: f32::NAN,
                ..NavAgent::default()
            },
        ),
        (
            "target",
            NavAgent {
                target: Some(Vec3::new(f32::NAN, 0.0, 0.0)),
                ..NavAgent::default()
            },
        ),
    ];
    for (field, nav) in cases {
        match nav.validate(id) {
            Err(NavError::Invalid { entity, message }) => {
                assert_eq!(entity, id);
                assert!(
                    message.contains(&format!("NavAgent.{field}")),
                    "{field}: {message}"
                );
            }
            other => panic!("{field}: {other:?}"),
        }
    }
}

/// A crowd agent: a `NavAgent` plus a `CrowdAgent`, with no controller (a surface agent).
fn crowd_agent(level: &mut Level, id: &str, at: Vec3, nav: NavAgent) -> EntityId {
    let id: EntityId = id.parse().unwrap();
    level.world.spawn_with_id(id).unwrap();
    let t = Transform {
        translation: at,
        ..Transform::default()
    };
    level.world.insert(id, t).unwrap();
    level.world.insert(id, nav).unwrap();
    level.world.insert(id, CrowdAgent::default()).unwrap();
    id
}

/// A plain agent with a controller, under a chosen id.
fn plain_agent(level: &mut Level, id: &str, at: Vec3, nav: NavAgent) -> EntityId {
    let id: EntityId = id.parse().unwrap();
    level.world.spawn_with_id(id).unwrap();
    let t = Transform {
        translation: at,
        ..Transform::default()
    };
    level.world.insert(id, t).unwrap();
    level.world.insert(id, nav).unwrap();
    let controller = CharacterController {
        radius: 0.25,
        ..CharacterController::default()
    };
    level.world.insert(id, controller).unwrap();
    level.world.insert(id, CharacterMotion::default()).unwrap();
    id
}

#[test]
fn crowd_agents_are_planned_with_their_corridor_in_the_shared_budget() {
    // Five agents wait from tick 0 in one queue, two plans a tick, by id: a, b (crowd) on tick 0,
    // c, d (crowd) on tick 1, e on tick 2. A block in the middle splits the floor into several
    // polygons. Crowd agents need no controller and keep their polygon corridor, whose string
    // pulling gives back their waypoints; plain agents keep none.
    let mut level = floor();
    level.block(
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.6, 1.0, 3.0),
        Quat::IDENTITY,
    );
    let to = |z: f32| going(Vec3::new(5.0, 0.0, z), 3.0);
    let a = plain_agent(&mut level, IDS[0], Vec3::new(-5.0, 0.0, -1.0), to(1.0));
    let b = crowd_agent(&mut level, IDS[1], Vec3::new(-5.0, 0.0, -0.5), to(0.5));
    let c = plain_agent(&mut level, IDS[2], Vec3::new(-5.0, 0.0, 0.0), to(0.0));
    let d = crowd_agent(&mut level, IDS[3], Vec3::new(-5.0, 0.0, 0.5), to(-0.5));
    let e = plain_agent(&mut level, IDS[4], Vec3::new(-5.0, 0.0, 1.0), to(-1.0));
    let idle = crowd_agent(
        &mut level,
        "018bcfe5-6800-7000-a424-0000000000f0",
        Vec3::new(4.0, 0.0, 4.0),
        NavAgent::default(),
    );
    let mut rig = Rig::new(level, 2);
    let mesh = rig
        .world
        .resource::<NavMeshes>()
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .1
        .clone();
    use NavStatus::{Moving, Waiting};
    let statuses = |rig: &Rig| -> Vec<NavStatus> {
        [a, b, c, d, e]
            .iter()
            .map(|id| rig.state(*id).status)
            .collect()
    };
    rig.step(1);
    assert_eq!(statuses(&rig), [Moving, Moving, Waiting, Waiting, Waiting]);
    // An idle crowd agent is placed on the navmesh (its polygon, outside the budget), so it can be
    // pushed and kept on the navmesh before it ever walks.
    let placed = rig.state(idle);
    let under = mesh.nearest_poly(Vec3::new(4.0, 0.0, 4.0)).unwrap().0;
    assert_eq!(placed.status, NavStatus::Idle);
    assert_eq!(
        (placed.corridor.clone(), placed.volume.is_some()),
        (vec![under], true)
    );
    rig.step(1);
    assert_eq!(statuses(&rig), [Moving, Moving, Moving, Moving, Waiting]);
    rig.step(1);
    assert_eq!(statuses(&rig), [Moving; 5]);
    for id in [a, c, e] {
        assert!(
            rig.state(id).corridor.is_empty(),
            "plain agents keep no corridor"
        );
    }
    for (id, start) in [
        (b, Vec3::new(-5.0, 0.0, -0.5)),
        (d, Vec3::new(-5.0, 0.0, 0.5)),
    ] {
        let s = rig.state(id);
        assert!(s.corridor.len() > 1, "around the block: {s:?}");
        let first = mesh.nearest_poly(start).expect("on the floor").0;
        let end = *s.waypoints.last().unwrap();
        let last = mesh.nearest_poly(end).expect("on the floor").0;
        assert_eq!((s.corridor[0], *s.corridor.last().unwrap()), (first, last));
        let pulled = mesh
            .straight_path(s.waypoints[0], end, &s.corridor, 64)
            .expect("pulls");
        assert_eq!(pulled, s.waypoints, "the corridor is the path's");
    }

    // Without a target a crowd agent's state resets and it is placed again on the polygon it
    // stands on (with its volume), so it can still be pushed and kept on the navmesh; a plain
    // agent just resets.
    let kept = rig.state(b).corridor[0];
    let volume = rig.state(b).volume;
    rig.agent(b).target = None;
    rig.agent(a).target = None;
    rig.step(1);
    let s = rig.state(b);
    assert_eq!(
        (s.status, s.corridor.clone(), s.volume),
        (NavStatus::Idle, vec![kept], volume)
    );
    assert_eq!(rig.state(a), NavAgentState::default());
}

#[test]
fn nav_follow_leaves_crowd_agents_to_the_crowd() {
    // A crowd agent with a controller (a physics agent) is planned but nav.follow never writes
    // its motion (only the crowd does), and being 2.5 m off its path doesn't replan it: its
    // corridor follows it instead. A plain agent next to it walks as before.
    let mut level = floor();
    let crowd = plain_agent(
        &mut level,
        IDS[0],
        Vec3::new(-5.0, 0.0, 0.0),
        going(Vec3::new(5.0, 0.0, 0.0), 3.0),
    );
    level.world.insert(crowd, CrowdAgent::default()).unwrap();
    let walker = plain_agent(
        &mut level,
        IDS[1],
        Vec3::new(-5.0, 0.0, 2.0),
        going(Vec3::new(5.0, 0.0, 2.0), 3.0),
    );
    let mut rig = Rig::new(level, 32);
    rig.step(10);
    let s = rig.state(crowd);
    assert_eq!(s.status, NavStatus::Moving, "{s:?}");
    assert_eq!(
        rig.world.get::<CharacterMotion>(crowd).unwrap().velocity,
        Vec3::ZERO,
        "nav.follow never wrote it"
    );
    assert!(close(rig.pos(crowd), Vec3::new(-5.0, 0.0, 0.0), EPS));
    assert!(
        close(rig.pos(walker), Vec3::new(-4.5, 0.0, 2.0), EPS),
        "{}",
        rig.pos(walker)
    );
    rig.teleport(crowd, Vec3::new(-5.0, 0.0, 2.5));
    rig.step(1);
    let after = rig.state(crowd);
    assert_eq!(
        (after.status, after.waiting_since),
        (NavStatus::Moving, s.waiting_since)
    );
}

#[test]
fn malformed_crowd_agents_are_descriptive_errors() {
    let id: EntityId = IDS[0].parse().unwrap();
    assert!(CrowdAgent::default().validate(id).is_ok());
    let off = CrowdAgent {
        avoidance: None,
        ..CrowdAgent::default()
    };
    assert!(off.validate(id).is_ok(), "avoidance can be off");
    let avoid = |f: fn(&mut CrowdAvoidance)| {
        let mut a = CrowdAvoidance::default();
        f(&mut a);
        CrowdAgent {
            avoidance: Some(a),
            ..CrowdAgent::default()
        }
    };
    let cases: Vec<(&str, CrowdAgent)> = vec![
        (
            "CrowdAgent.max_acceleration",
            CrowdAgent {
                max_acceleration: 0.0,
                ..CrowdAgent::default()
            },
        ),
        (
            "CrowdAgent.collision_query_range",
            CrowdAgent {
                collision_query_range: f32::NAN,
                ..CrowdAgent::default()
            },
        ),
        (
            "CrowdAgent.path_optimization_range",
            CrowdAgent {
                path_optimization_range: -1.0,
                ..CrowdAgent::default()
            },
        ),
        (
            "CrowdAgent.separation_weight",
            CrowdAgent {
                separation_weight: f32::INFINITY,
                ..CrowdAgent::default()
            },
        ),
        (
            "CrowdAvoidance.velocity_bias",
            avoid(|a| a.velocity_bias = 1.5),
        ),
        (
            "CrowdAvoidance.weight_desired",
            avoid(|a| a.weight_desired = -1.0),
        ),
        (
            "CrowdAvoidance.weight_current",
            avoid(|a| a.weight_current = f32::NAN),
        ),
        (
            "CrowdAvoidance.weight_side",
            avoid(|a| a.weight_side = -0.5),
        ),
        (
            "CrowdAvoidance.weight_toi",
            avoid(|a| a.weight_toi = f32::INFINITY),
        ),
        ("CrowdAvoidance.horizon", avoid(|a| a.horizon = 0.0)),
        ("CrowdAvoidance.divs", avoid(|a| a.divs = 33)),
        ("CrowdAvoidance.divs", avoid(|a| a.divs = 0)),
        ("CrowdAvoidance.rings", avoid(|a| a.rings = 5)),
        ("CrowdAvoidance.depth", avoid(|a| a.depth = 0)),
        ("CrowdAvoidance.depth", avoid(|a| a.depth = 9)),
    ];
    for (field, crowd) in cases {
        match crowd.validate(id) {
            Err(NavError::Invalid { entity, message }) => {
                assert_eq!(entity, id);
                assert!(message.contains(field), "{field}: {message}");
            }
            other => panic!("{field}: {other:?}"),
        }
    }
}
