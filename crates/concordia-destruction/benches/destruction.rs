//! Destruction benchmark (M8 8.5b, `cargo bench -p concordia-destruction --bench destruction`;
//! results go in `BASELINE.md` once recorded on the reference machine): a 2,000-brick wall (50 × 40,
//! bottom row anchored) crumbles at once at 120 Hz. `wall_breaks` times the ticks in which every
//! bond breaks and 2,000 bodies appear; `debris_falls` times one second of the debris after.
//! The target: a tick within 8.3 ms (120 Hz), with the render frame on top at 60 fps.
#![allow(missing_docs)]

use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::math::Vec3;
use concordia_core::time::TickRate;
use concordia_destruction::{
    Breakable, DestructionSettings, Falloff, Field, FieldEffect, FieldShape, Loaded, MemorySource,
    destruction_system,
};
use concordia_fracture::{Fracture, Piece, Slot, StructureSpec, structure};
use concordia_physics::{Collider, ColliderShape, PhysicsRuntime, PhysicsSettings, RigidBody};
use concordia_scene::builtin::Transform;
use concordia_terra::{Phase, Schedule, SimLoop, SystemCtx, World, system};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

const RECIPE: &str = "breakables/wall.breakable.ron";
const WIDE: u32 = 50;
const HIGH: u32 = 40;
const BRICK: [f32; 3] = [0.2, 0.1, 0.2];

/// One brick's box at grid cell (x, y): faces against a neighbour are interior (bonded).
fn brick(x: u32, y: u32) -> Piece {
    let lo = [x as f32 * BRICK[0], y as f32 * BRICK[1], 0.0];
    let positions: Vec<[f32; 3]> = (0..8u32)
        .map(|i| [0, 1, 2].map(|a| lo[a] + if i & (1 << a) == 0 { 0.0 } else { BRICK[a] }))
        .collect();
    // Per face: two counter-clockwise triangles (from outside) and whether a neighbour is there.
    let faces = [
        ([[0, 4, 6], [0, 6, 2]], x > 0),
        ([[1, 3, 7], [1, 7, 5]], x + 1 < WIDE),
        ([[0, 1, 5], [0, 5, 4]], y > 0),
        ([[2, 6, 7], [2, 7, 3]], y + 1 < HIGH),
        ([[0, 2, 3], [0, 3, 1]], false),
        ([[4, 5, 7], [4, 7, 6]], false),
    ];
    let mut triangles = Vec::new();
    let mut materials = Vec::new();
    for (pair, inner) in faces {
        triangles.extend(pair);
        let slot = if inner { Slot::Interior } else { Slot::Surface };
        materials.extend([slot; 2]);
    }
    Piece {
        cell: y * WIDE + x,
        positions,
        triangles,
        materials,
        volume: f64::from(BRICK[0] * BRICK[1] * BRICK[2]),
    }
}

fn source() -> Arc<MemorySource> {
    let fracture = Fracture {
        pieces: (0..HIGH)
            .flat_map(|y| (0..WIDE).map(move |x| brick(x, y)))
            .collect(),
    };
    let spec = StructureSpec {
        density: 2000.0,
        strength: 1.0e6,
        levels: Vec::new(),
        edges: Vec::new(),
    };
    let structure = structure(&fracture, &spec).expect("the wall's bonds");
    let loaded = Loaded {
        fracture,
        structure,
        looks: None,
    };
    Arc::new(MemorySource::default().with(RECIPE, loaded))
}

/// Ground, the wall and a crumble field over all of it, scheduled with physics and destruction.
fn scene(source: Arc<MemorySource>) -> (World, SimLoop) {
    let mut world = World::new();
    let mut ids = DeterministicIdGen::new(3);
    let mut spawn = |world: &mut World, t: Vec3| -> EntityId {
        let id = world.spawn(&mut ids).unwrap();
        world
            .insert(
                id,
                Transform {
                    translation: t,
                    ..Transform::default()
                },
            )
            .unwrap();
        id
    };
    let ground = spawn(&mut world, Vec3::new(0.0, -0.5, 0.0));
    world.insert(ground, RigidBody::fixed()).unwrap();
    let floor = ColliderShape::Box {
        half_extents: Vec3::new(50.0, 0.5, 50.0),
    };
    world.insert(ground, Collider::new(floor)).unwrap();
    let wall = spawn(&mut world, Vec3::ZERO);
    world.insert(wall, RigidBody::dynamic()).unwrap();
    let breakable = Breakable {
        recipe: RECIPE.into(),
        anchors: (0..WIDE).collect(),
        impact_radius: 0.2,
    };
    world.insert(wall, breakable).unwrap();
    let field = spawn(&mut world, Vec3::new(5.0, 2.0, 0.0));
    let crumble = Field {
        shape: FieldShape::Sphere { radius: 20.0 },
        falloff: Falloff::None,
        effect: FieldEffect::Crumble,
        magnitude: 0.0,
        duration: 0.0,
    };
    world.insert(field, crumble).unwrap();
    let mut physics =
        PhysicsRuntime::new(PhysicsSettings::default(), NonZeroUsize::new(8).unwrap()).unwrap();
    let mut schedule = Schedule::new();
    schedule.add_in(
        Phase::Physics,
        system(
            "physics.step",
            move |world: &mut World, ctx: &SystemCtx<'_>| {
                physics.tick(world, ctx.dt).unwrap();
            },
        ),
    );
    let settings = DestructionSettings {
        breaks_per_tick: 8192,
        max_debris: 4096,
        ..DestructionSettings::default()
    };
    schedule.add_in(Phase::PostPhysics, destruction_system(source, settings));
    let rate = TickRate::from_hz(120).unwrap();
    let sim = SimLoop::new(rate, schedule, &mut world).unwrap();
    (world, sim)
}

/// How many fragments there are.
fn parts(world: &mut World) -> usize {
    let mut n = 0;
    if let Ok(mut q) = world.query::<&concordia_destruction::Fragment>() {
        q.for_each(world, |_, _| n += 1);
    }
    n
}

fn bench(c: &mut Criterion) {
    let source = source();
    let jobs = JobSystem::new(NonZeroUsize::new(8).unwrap()).unwrap();
    let mut group = c.benchmark_group("destruction");
    group.sample_size(10);
    group.bench_function("wall_breaks", |b| {
        b.iter_batched(
            || scene(source.clone()),
            |(mut world, mut sim)| sim.step(&mut world, &jobs, 3),
            BatchSize::PerIteration,
        );
    });
    group.bench_function("debris_falls", |b| {
        b.iter_batched(
            || {
                let (mut world, mut sim) = scene(source.clone());
                sim.step(&mut world, &jobs, 3);
                assert_eq!(
                    parts(&mut world),
                    (WIDE * HIGH) as usize,
                    "every brick apart"
                );
                (world, sim)
            },
            |(mut world, mut sim)| sim.step(&mut world, &jobs, 120),
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
