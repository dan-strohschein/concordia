//! Simulation LOD benchmarks (M2 phase 2.4b.1, `just bench -p concordia-terra --bench lod`);
//! results in `BASELINE.md`. One tick of a processor integrating positions over a grid of
//! entities, at full rate (before LOD) and LOD-aware with one viewer at the grid's center, and
//! `terra.lod` alone.
#![allow(missing_docs)]

use std::num::NonZeroUsize;

use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
use concordia_core::jobs::JobSystem;
use concordia_core::time::TickRate;
use concordia_reflect::Reflect;
use concordia_terra::{
    LodProfile, LodViewer, Phase, Schedule, SimLoop, SystemCtx, View, World, lod_system, processor,
};
use criterion::{Criterion, criterion_group, criterion_main};
use serde::{Deserialize, Serialize};

const CHUNK: NonZeroUsize = NonZeroUsize::new(1024).unwrap();

/// Position.
#[derive(Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
struct Pos([f32; 3]);

/// Velocity.
#[derive(Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
struct Vel([f32; 3]);

fn position(world: &World, id: EntityId) -> Option<[f32; 3]> {
    world.get::<Pos>(id).map(|p| p.0)
}

/// `n` entities on a 1 m grid (with profiles if `lod`), plus a viewer at its center.
fn world(n: usize, lod: bool) -> World {
    let mut world = World::new();
    world.register_shared::<LodProfile>().unwrap();
    let side = (n as f32).sqrt().ceil() as usize;
    let profile = LodProfile::new([20.0, 60.0, 120.0], [2, 4, 8], 3.0).unwrap();
    let mut source = DeterministicIdGen::new(5);
    let center = side as f32 / 2.0;
    let viewer = source.next_id().unwrap();
    world.spawn_with_id(viewer).unwrap();
    world.insert(viewer, Pos([center, 0.0, center])).unwrap();
    world.insert(viewer, LodViewer).unwrap();
    for i in 0..n {
        let id = source.next_id().unwrap();
        world.spawn_with_id(id).unwrap();
        let p = [(i % side) as f32, 0.0, (i / side) as f32];
        world.insert(id, Pos(p)).unwrap();
        world.insert(id, Vel([1.0, 0.0, 0.5])).unwrap();
        if lod {
            world.insert(id, profile).unwrap();
        }
    }
    world
}

fn schedule(lod: bool) -> Schedule {
    let mut schedule = Schedule::new();
    let mut decl = processor("integrate", Phase::PostPhysics)
        .writes::<Pos>()
        .reads::<Vel>();
    if lod {
        schedule.add_in(Phase::PrePhysics, lod_system(position));
        decl = decl.lod();
    }
    schedule.add_processor(decl.run(|view: &mut View<'_>, ctx: &SystemCtx<'_>| {
        if let Ok(query) = view.query::<(&mut Pos, &Vel)>() {
            query.par_for_each_due(ctx.jobs, CHUNK, |_, due, (p, v)| {
                for k in 0..3 {
                    p.0[k] += v.0[k] * due.dt;
                }
            });
        }
    }));
    schedule
}

fn ticks(c: &mut Criterion) {
    for n in [1_000usize, 100_000] {
        for threads in [1usize, 8] {
            let jobs = JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap();
            for lod in [false, true] {
                let mut world = world(n, lod);
                let mut sim = SimLoop::new(TickRate::HZ_60, schedule(lod), &mut world).unwrap();
                sim.step(&mut world, &jobs, 8);
                let kind = if lod { "lod" } else { "full" };
                let name = format!("{kind}_tick_{}_{threads}_threads", label(n));
                c.bench_function(&name, |b| b.iter(|| sim.step(&mut world, &jobs, 1)));
            }
            let mut world = world(n, true);
            let mut alone = Schedule::new();
            alone.add_in(Phase::PrePhysics, lod_system(position));
            let mut sim = SimLoop::new(TickRate::HZ_60, alone, &mut world).unwrap();
            sim.step(&mut world, &jobs, 8);
            let name = format!("terra_lod_{}_{threads}_threads", label(n));
            c.bench_function(&name, |b| b.iter(|| sim.step(&mut world, &jobs, 1)));
        }
    }
}

fn label(n: usize) -> String {
    if n >= 1000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

criterion_group!(benches, ticks);
criterion_main!(benches);
