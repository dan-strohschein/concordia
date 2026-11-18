//! ECS micro-benchmarks (`just bench`); results recorded in `BASELINE.md`.
#![allow(missing_docs)]

use std::hint::black_box;
use std::num::NonZeroUsize;

use concordia_core::id::{DeterministicIdGen, EntityId, IdSource};
use concordia_core::jobs::JobSystem;
use concordia_ecs::World;
use concordia_reflect::Reflect;
use criterion::{Criterion, criterion_group, criterion_main};
use serde::{Deserialize, Serialize};

/// Position.
#[derive(Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
struct Pos([f32; 3]);

/// Velocity.
#[derive(Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
struct Vel([f32; 3]);

/// Tag.
#[derive(Clone, PartialEq, Serialize, Deserialize, Reflect)]
struct Tag;

fn ids(n: usize) -> Vec<EntityId> {
    let mut source = DeterministicIdGen::new(3);
    (0..n).map(|_| source.next_id().unwrap()).collect()
}

fn world(n: usize) -> (World, Vec<EntityId>) {
    let mut world = World::new();
    let ids = ids(n);
    for &id in &ids {
        world
            .spawn_with_id(id)
            .unwrap()
            .insert(Pos::default())
            .unwrap()
            .insert(Vel([1.0; 3]))
            .unwrap();
    }
    (world, ids)
}

fn spawn(c: &mut Criterion) {
    let ids = ids(10_000);
    c.bench_function("spawn_10k_two_components", |b| {
        b.iter(|| {
            let mut world = World::new();
            for &id in &ids {
                world
                    .spawn_with_id(id)
                    .unwrap()
                    .insert(Pos::default())
                    .unwrap()
                    .insert(Vel::default())
                    .unwrap();
            }
            black_box(world.len())
        })
    });
}

fn iterate(c: &mut Criterion) {
    let (mut world, _) = world(100_000);
    let mut read = world.query::<&Pos>().unwrap();
    c.bench_function("iter_100k_read_one", |b| {
        b.iter(|| {
            let mut sum = 0.0;
            read.for_each(&mut world, |_, p| sum += p.0[0]);
            black_box(sum)
        })
    });
    let mut write = world.query::<(&mut Pos, &mut Vel)>().unwrap();
    c.bench_function("iter_100k_write_two", |b| {
        b.iter(|| {
            write.for_each(&mut world, |_, (p, v)| {
                p.0[0] += v.0[0];
                v.0[1] += 1.0;
            })
        })
    });
}

fn par_iterate(c: &mut Criterion) {
    let (mut world, _) = world(1_000_000);
    let chunk = NonZeroUsize::new(4096).unwrap();
    for threads in [1, 4, 8] {
        let jobs = JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap();
        let mut query = world.query::<(&mut Pos, &mut Vel)>().unwrap();
        c.bench_function(&format!("par_iter_1m_write_two_{threads}_threads"), |b| {
            b.iter(|| {
                query.par_for_each(&mut world, &jobs, chunk, |_, (p, v)| {
                    p.0[0] += v.0[0];
                    v.0[1] += 1.0;
                })
            })
        });
    }
}

fn archetype_moves(c: &mut Criterion) {
    let (mut world, ids) = world(10_000);
    c.bench_function("add_remove_component_10k", |b| {
        b.iter(|| {
            for &id in &ids {
                world.insert(id, Tag).unwrap();
            }
            for &id in &ids {
                world.remove::<Tag>(id).unwrap();
            }
        })
    });
}

fn hashing(c: &mut Criterion) {
    let (world, _) = world(10_000);
    c.bench_function("world_hash_10k", |b| {
        b.iter(|| black_box(world.state_hash().unwrap()))
    });
}

criterion_group!(
    benches,
    spawn,
    iterate,
    par_iterate,
    archetype_moves,
    hashing
);
criterion_main!(benches);
