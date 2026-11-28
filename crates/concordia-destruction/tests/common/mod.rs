//! The breaking tests' harness: a slab of four slices, physics and destruction on a
//! schedule, scripted blows, and extra entities (fields) spawned by the test.
#![allow(dead_code, missing_docs)]

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::math::Vec3;
use concordia_core::time::TickRate;
use concordia_destruction::{
    Breakable, DestructionSettings, Fragment, Loaded, MemorySource, destruction_system,
};
use concordia_fracture::{Mesh, Pattern, StructureSpec, fracture, structure};
use concordia_physics::{
    Collider, ColliderShape, ContactImpact, PhysicsRuntime, PhysicsSettings, RigidBody,
};
use concordia_scene::builtin::{Transform, Velocity};
use concordia_terra::{Phase, Schedule, SimLoop, SystemCtx, World, system};

pub const RECIPE: &str = "breakables/slab.breakable.ron";

pub fn slab() -> Mesh {
    let mut positions = Vec::new();
    for i in 0..8u32 {
        let s = |bit: u32, hi: f32| if i & bit == 0 { 0.0 } else { hi };
        positions.push([s(1, 2.0), s(2, 1.0), s(4, 1.0)]);
    }
    let triangles = vec![
        [0, 4, 6],
        [0, 6, 2],
        [1, 3, 7],
        [1, 7, 5],
        [0, 1, 5],
        [0, 5, 4],
        [2, 6, 7],
        [2, 7, 3],
        [0, 2, 3],
        [0, 3, 1],
        [4, 5, 7],
        [4, 7, 6],
    ];
    Mesh {
        positions,
        triangles,
    }
}

pub fn source() -> Arc<MemorySource> {
    let f = fracture(
        &slab(),
        &Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 4,
        },
        0,
    )
    .unwrap();
    let spec = StructureSpec {
        density: 1000.0,
        strength: 100.0,
        levels: Vec::new(),
        edges: Vec::new(),
    };
    let s = structure(&f, &spec).unwrap();
    Arc::new(MemorySource::default().with(
        RECIPE,
        Loaded {
            fracture: f,
            structure: s,
        },
    ))
}

/// A blow: on tick `tick`, an impact of `strength` at `local` (in the slab's mesh space).
#[derive(Clone, Copy)]
pub struct Blow {
    pub tick: u64,
    pub local: Vec3,
    pub strength: f32,
}

pub struct Run {
    pub threads: usize,
    pub gravity: bool,
    pub anchors: Vec<u32>,
    pub radius: f32,
    pub breaks_per_tick: u32,
    /// The debris settings (`breaks_per_tick` comes from the field above).
    pub debris: DestructionSettings,
    pub velocity: Velocity,
    pub blows: Vec<Blow>,
    /// Spawns more entities (fields) after the slab.
    pub setup: Option<fn(&mut World)>,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            threads: 1,
            gravity: false,
            anchors: Vec::new(),
            radius: 0.3,
            breaks_per_tick: 64,
            debris: DestructionSettings::default(),
            velocity: Velocity::default(),
            blows: Vec::new(),
            setup: None,
        }
    }
}

pub const START: Vec3 = Vec3::new(0.0, 3.0, 0.0);

pub fn root() -> EntityId {
    "0193a1c2-7e40-7000-8000-0000000000aa".parse().unwrap()
}

/// Runs `ticks` ticks of physics plus destruction and returns the world.
pub fn run(r: &Run, ticks: u32) -> World {
    let mut settings = PhysicsSettings::default();
    if !r.gravity {
        settings.gravity = Vec3::ZERO;
    }
    let mut physics = PhysicsRuntime::new(settings, NonZeroUsize::new(r.threads).unwrap()).unwrap();
    let mut world = World::new();
    let mut ids = DeterministicIdGen::new(5);
    if r.gravity {
        let ground = world.spawn(&mut ids).unwrap();
        world
            .insert(
                ground,
                Transform {
                    translation: Vec3::new(0.0, -0.5, 0.0),
                    ..Transform::default()
                },
            )
            .unwrap();
        world.insert(ground, RigidBody::fixed()).unwrap();
        world
            .insert(
                ground,
                Collider::new(ColliderShape::Box {
                    half_extents: Vec3::new(30.0, 0.5, 30.0),
                }),
            )
            .unwrap();
    }
    world.spawn_with_id(root()).unwrap();
    world
        .insert(
            root(),
            Transform {
                translation: START,
                ..Transform::default()
            },
        )
        .unwrap();
    world
        .insert(
            root(),
            RigidBody {
                linear_damping: 0.0,
                angular_damping: 0.0,
                ..RigidBody::dynamic()
            },
        )
        .unwrap();
    world.insert(root(), r.velocity).unwrap();
    world
        .insert(
            root(),
            Breakable {
                recipe: RECIPE.into(),
                anchors: r.anchors.clone(),
                impact_radius: r.radius,
            },
        )
        .unwrap();
    if let Some(setup) = r.setup {
        setup(&mut world);
    }
    let mut schedule = Schedule::new();
    let blows = r.blows.clone();
    schedule.add_in(
        Phase::PrePhysics,
        system(
            "test.blows",
            move |world: &mut World, ctx: &SystemCtx<'_>| {
                for b in blows.iter().filter(|b| b.tick == ctx.tick.0) {
                    // Hit whichever fragment holds the point, in world space.
                    let mut targets: Vec<(EntityId, Transform)> = Vec::new();
                    if let Ok(mut q) = world.query::<(&Fragment, &Transform)>() {
                        q.for_each(world, |id, (_, t)| targets.push((id, *t)));
                    }
                    targets.sort_by_key(|t| t.0);
                    for (id, t) in targets {
                        let point = t.translation + t.rotation * b.local;
                        world.signal(
                            id,
                            ContactImpact {
                                other: id,
                                strength: b.strength,
                                point,
                            },
                        );
                    }
                }
            },
        ),
    );
    schedule.add_in(
        Phase::Physics,
        system(
            "physics.step",
            move |world: &mut World, ctx: &SystemCtx<'_>| {
                physics.tick(world, ctx.dt).unwrap();
            },
        ),
    );
    schedule.add_in(
        Phase::PostPhysics,
        destruction_system(
            source(),
            DestructionSettings {
                breaks_per_tick: r.breaks_per_tick,
                ..r.debris
            },
        ),
    );
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    let jobs = JobSystem::new(NonZeroUsize::new(r.threads).unwrap()).unwrap();
    sim.step(&mut world, &jobs, ticks);
    world
}

/// Every fragment, by its sorted pieces.
pub fn fragments(world: &mut World) -> BTreeMap<Vec<u32>, EntityId> {
    let mut out = BTreeMap::new();
    if let Ok(mut q) = world.query::<&Fragment>() {
        q.for_each(world, |id, f| {
            out.insert(f.pieces.clone(), id);
        });
    }
    out
}

/// A blow on the middle cut (x = 1), strong enough to break that bond only.
pub fn middle(tick: u64) -> Blow {
    Blow {
        tick,
        local: Vec3::new(1.0, 0.5, 0.5),
        strength: 150.0,
    }
}
