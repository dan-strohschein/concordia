//! Crowd fixtures: a floor and its navmesh, and a rig that runs nav's and the crowd's processors
//! on a `SimLoop` (no physics: surface agents move themselves).
#![allow(dead_code, missing_docs)]

pub mod engine;

use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;

use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::math::Vec3;
use concordia_core::time::TickRate;
use concordia_nav::{
    AgentConfig, CrowdAgent, NavAgent, NavAgentState, NavArrived, NavInput, NavMesh, NavMeshVolume,
    NavMeshes, NavPathFailed, add_agent_processors, build_tiles, insert_agent_states,
};
use concordia_physics::{BodyKind, CharacterController, Collider, ColliderShape, RigidBody};
use concordia_scene::builtin::{Transform, lod_position};
use concordia_terra::{
    LodProfile, LodViewer, Phase, Schedule, SimLod, SimLoop, World, WorldHash, lod_system,
};
use concordia_terra_crowd::{CrowdState, add_crowd_processors, insert_crowd_states};

/// The walkable surface of a floor whose top is y = 0 (2.5a: a span is at least one cell tall).
pub const SURFACE: f32 = 0.2;

/// 0.25 m cells and agent radius, outlines exact to the cell, 32 m tiles.
pub fn volume_settings(half: Vec3) -> NavMeshVolume {
    NavMeshVolume {
        half_extents: half,
        agent_radius: 0.25,
        agent_height: 1.8,
        agent_max_climb: 0.3,
        agent_max_slope_deg: 45.0,
        cell_size: 0.25,
        cell_height: 0.2,
        tile_size: 128,
        edge_max_error: 0.5,
    }
}

pub struct Level {
    pub world: World,
    ids: DeterministicIdGen,
    pub volume: Option<EntityId>,
}

impl Level {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            ids: DeterministicIdGen::new(11),
            volume: None,
        }
    }

    pub fn entity(&mut self, at: Vec3) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        let t = Transform {
            translation: at,
            ..Transform::default()
        };
        self.world.insert(id, t).unwrap();
        id
    }

    /// A static box standing on y = 0 (or, with `top` = 0, a floor slab under it).
    pub fn static_box(&mut self, center: Vec3, half: Vec3) -> EntityId {
        let id = self.entity(center);
        let body = RigidBody {
            kind: BodyKind::Static,
            ..RigidBody::default()
        };
        self.world.insert(id, body).unwrap();
        let shape = ColliderShape::Box { half_extents: half };
        self.world.insert(id, Collider::new(shape)).unwrap();
        id
    }

    /// A floor with its top at y = 0 and its edges at ±(`half` + 0.1), 0.1 m past a cell boundary
    /// (2.5a's fixtures): the ledge ring and one eroded cell leave it walkable to ±(`half` − 0.25),
    /// at y = 0.2. A volume 2 m wider holds it.
    pub fn floor(half_x: f32, half_z: f32) -> Self {
        let mut level = Self::new();
        level.static_box(
            Vec3::new(0.0, -0.5, 0.0),
            Vec3::new(half_x + 0.1, 0.5, half_z + 0.1),
        );
        let volume = level.entity(Vec3::ZERO);
        let settings = volume_settings(Vec3::new(half_x + 2.0, 3.0, half_z + 2.0));
        level.world.insert(volume, settings).unwrap();
        level.volume = Some(volume);
        level
    }

    /// A surface crowd agent (no controller) at `at` with `nav`.
    pub fn crowd_agent(&mut self, at: Vec3, nav: NavAgent) -> EntityId {
        self.crowd_agent_with(at, nav, CrowdAgent::default())
    }

    pub fn crowd_agent_with(&mut self, at: Vec3, nav: NavAgent, crowd: CrowdAgent) -> EntityId {
        let id = self.entity(at);
        self.world.insert(id, nav).unwrap();
        self.world.insert(id, crowd).unwrap();
        id
    }

    /// A character the crowd treats as solid (a controller, no agent).
    pub fn character(&mut self, at: Vec3, radius: f32) -> EntityId {
        let id = self.entity(at);
        let controller = CharacterController {
            radius,
            ..CharacterController::default()
        };
        self.world.insert(id, controller).unwrap();
        id
    }

    /// A `LodViewer` at `at`, and `profile` on each of `agents`.
    pub fn lod(&mut self, at: Vec3, agents: &[EntityId], profile: LodProfile) -> EntityId {
        let viewer = self.entity(at);
        self.world.insert(viewer, LodViewer).unwrap();
        let _ = self.world.register_shared::<LodProfile>();
        for &id in agents {
            self.world.insert(id, profile).unwrap();
        }
        viewer
    }

    pub fn build(&self) -> NavMesh {
        let input = NavInput::gather(&self.world, self.volume.expect("a volume")).expect("gathers");
        let data = build_tiles(&input, NonZeroUsize::new(4).unwrap()).expect("builds");
        NavMesh::from_data(data).expect("loads")
    }
}

/// An agent going to `to` at `speed`, sized like the fixture volume's agent.
pub fn going(to: Vec3, speed: f32) -> NavAgent {
    NavAgent {
        target: Some(to),
        max_speed: speed,
        radius: 0.25,
        ..NavAgent::default()
    }
}

/// An idle agent sized like the fixture volume's agent.
pub fn idle() -> NavAgent {
    NavAgent {
        radius: 0.25,
        ..NavAgent::default()
    }
}

/// Nav's and the crowd's processors on a `SimLoop` at 60 Hz.
pub struct Rig {
    pub world: World,
    sim: SimLoop,
    jobs: JobSystem,
    pub mesh: Arc<NavMesh>,
}

impl Rig {
    pub fn new(level: Level) -> Self {
        Self::with(level, 3, |_| {})
    }

    /// `extra` is registered first (so a test system in `PrePhysics` runs before the crowd).
    pub fn with(mut level: Level, threads: usize, extra: impl FnOnce(&mut Schedule)) -> Self {
        let mesh = Arc::new(level.build());
        let volume = level.volume.expect("a volume");
        level
            .world
            .insert_resource(NavMeshes::new([(volume, mesh.clone())]));
        insert_agent_states(&mut level.world);
        insert_crowd_states(&mut level.world);
        let mut schedule = Schedule::new();
        extra(&mut schedule);
        add_agent_processors(
            &mut schedule,
            AgentConfig {
                plans_per_tick: NonZeroU32::new(64).unwrap(),
            },
        );
        add_crowd_processors(&mut schedule);
        let mut world = level.world;
        let sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
        Self {
            world,
            sim,
            jobs: JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap(),
            mesh,
        }
    }

    /// Like [`Rig::with`], with `terra.lod` first in `PrePhysics` (as the scene's schedule has it).
    pub fn with_lod(level: Level, threads: usize) -> Self {
        Self::with(level, threads, |schedule| {
            schedule.add_in(Phase::PrePhysics, lod_system(lod_position));
        })
    }

    pub fn lod(&self, id: EntityId) -> Option<SimLod> {
        self.world.get::<SimLod>(id).copied()
    }

    pub fn step(&mut self, ticks: u32) {
        self.sim.step(&mut self.world, &self.jobs, ticks);
    }

    pub fn hash(&self) -> WorldHash {
        self.world.state_hash().unwrap()
    }

    pub fn pos(&self, id: EntityId) -> Vec3 {
        self.world.get::<Transform>(id).unwrap().translation
    }

    pub fn nav(&self, id: EntityId) -> NavAgentState {
        self.world.get::<NavAgentState>(id).unwrap().clone()
    }

    pub fn crowd(&self, id: EntityId) -> CrowdState {
        self.world.get::<CrowdState>(id).unwrap().clone()
    }

    pub fn arrived(&self) -> Vec<(EntityId, NavArrived)> {
        self.world
            .pending_signals::<NavArrived>()
            .into_iter()
            .map(|(id, s)| (id, *s))
            .collect()
    }

    pub fn failed(&self) -> Vec<(EntityId, NavPathFailed)> {
        self.world
            .pending_signals::<NavPathFailed>()
            .into_iter()
            .map(|(id, s)| (id, s.clone()))
            .collect()
    }
}

/// Lane `k`'s pair in the 1,000-agent field: (start, target) for each.
pub fn lane(k: usize) -> [(Vec3, Vec3); 2] {
    let (col, row) = ((k % 25) as f32, (k / 25) as f32);
    let (x, z) = (-96.0 + 8.0 * col, -19.0 + 2.0 * row);
    let offset = 0.1 + 0.3 * ((k % 7) as f32) / 6.0;
    [
        (Vec3::new(x - 3.0, 0.0, z), Vec3::new(x + 3.0, 0.0, z)),
        (
            Vec3::new(x + 3.0, 0.0, z + offset),
            Vec3::new(x - 3.0, 0.0, z + offset),
        ),
    ]
}

/// The steady-state overlap bound of `crowd.rs`'s header for a closing speed of `closing` m/s.
pub fn overlap_bound(closing: f32) -> f32 {
    let c = closing / 60.0;
    0.0081 * c / (1.0 - 0.0081)
}

pub fn close(a: Vec3, b: Vec3, eps: f32) -> bool {
    (a - b).abs().max_element() <= eps
}

/// The sideways distance from `a` to `b`.
pub fn sideways(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(a.x - b.x, 0.0, a.z - b.z).length()
}

/// The deepest overlap among discs at `points` with `radius`, by a sweep along x.
pub fn deepest_overlap(points: &[Vec3], radius: f32) -> f32 {
    let mut sorted: Vec<Vec3> = points.to_vec();
    sorted.sort_by(|a, b| a.x.total_cmp(&b.x));
    let reach = 2.0 * radius;
    let mut deepest = 0.0f32;
    for (i, a) in sorted.iter().enumerate() {
        for b in &sorted[i + 1..] {
            if b.x - a.x >= reach {
                break;
            }
            deepest = deepest.max(reach - sideways(*a, *b));
        }
    }
    deepest
}
