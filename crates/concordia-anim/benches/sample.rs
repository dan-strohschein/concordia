//! Sampling throughput (`just bench -p concordia-anim`); results recorded in `BASELINE.md`.
#![allow(missing_docs)]

use std::hint::black_box;
use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_anim::{AnimLibrary, AnimPlayer, BoundClip, Joint, Skeleton, add_processors};
use concordia_assets::{AssetRef, Channel, ChannelPath, ClipData, Interpolation, Skinned};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::math::{Mat4, Quat, Vec3};
use concordia_core::time::TickRate;
use concordia_scene::builtin::Transform;
use concordia_terra::{Schedule, SimLoop, World};
use criterion::{Criterion, criterion_group, criterion_main};

const BONES: usize = 65;
const CHARACTERS: usize = 1_000;
const KEYS: usize = 31;

fn skeleton() -> Skeleton {
    let joints = (0..BONES)
        .map(|j| Joint {
            name: format!("bone{j}"),
            parent: j.checked_sub(1),
            offset: Mat4::IDENTITY,
            rest: Transform {
                translation: Vec3::Y * 0.1,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            inverse_bind: Mat4::IDENTITY,
        })
        .collect();
    Skeleton::new(joints).unwrap()
}

/// One second at 30 fps: translation, rotation and scale keys on every bone.
fn clip() -> ClipData {
    let times: Vec<f32> = (0..KEYS).map(|k| k as f32 / 30.0).collect();
    let target: EntityId = "018bcfe5-6800-7000-a424-6c55076b137f".parse().unwrap();
    let channel = |j: usize, path: ChannelPath, values: Vec<f32>| Channel {
        target,
        target_name: format!("bone{j}"),
        path,
        interpolation: Interpolation::Linear,
        times: times.clone(),
        values,
    };
    let mut channels = Vec::new();
    for j in 0..BONES {
        let t = times.iter().flat_map(|t| [*t, 0.1, 0.0]).collect();
        let r = times
            .iter()
            .flat_map(|t| Quat::from_rotation_x(t + j as f32 * 0.01).to_array())
            .collect();
        let s = times.iter().flat_map(|t| [1.0 + t; 3]).collect();
        channels.push(channel(j, ChannelPath::Translation, t));
        channels.push(channel(j, ChannelPath::Rotation, r));
        channels.push(channel(j, ChannelPath::Scale, s));
    }
    ClipData {
        name: "bench".into(),
        duration: 1.0,
        frame_rate: Some(30.0),
        channels,
    }
}

fn sample(c: &mut Criterion) {
    let skeleton = skeleton();
    let bound = BoundClip::bind(&clip(), &skeleton).unwrap();
    let mut poses = vec![skeleton.rest_pose(); CHARACTERS];
    c.bench_function("sample_1000_characters_65_bones", |b| {
        b.iter(|| {
            for (i, pose) in poses.iter_mut().enumerate() {
                skeleton.reset(pose);
                bound.sample(black_box(i as f32 * 0.00097), pose).unwrap();
            }
        });
    });
}

fn tick(c: &mut Criterion) {
    let skin: AssetRef = "asset:018bcfe5-6800-7000-a424-6c55076b137f#skin/0"
        .parse()
        .unwrap();
    let clip_ref: AssetRef = "asset:018bcfe5-6800-7000-a424-6c55076b137f#clip/bench"
        .parse()
        .unwrap();
    let skeleton = skeleton();
    let bound = BoundClip::bind(&clip(), &skeleton).unwrap();
    let mut library = AnimLibrary::new();
    library.insert_clip(clip_ref.clone(), skin.clone(), bound);
    library.insert_skeleton(skin.clone(), skeleton);
    let library = Arc::new(library);
    let mut world = World::new();
    let mut ids = DeterministicIdGen::new(3);
    for _ in 0..CHARACTERS {
        let id = world.spawn(&mut ids).unwrap();
        world.insert(id, Skinned { skin: skin.clone() }).unwrap();
        world.insert(id, AnimPlayer::new(clip_ref.clone())).unwrap();
    }
    concordia_anim::insert_rest_poses(&library, &mut world);
    let mut schedule = Schedule::new();
    add_processors(&mut schedule, library);
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    for threads in [1, 8] {
        let jobs = JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap();
        c.bench_function(&format!("tick_1000_characters_{threads}_threads"), |b| {
            b.iter(|| sim.step(&mut world, &jobs, 1));
        });
    }
    c.bench_function("world_hash_1000_poses", |b| {
        b.iter(|| black_box(world.state_hash().unwrap()));
    });
}

criterion_group!(benches, sample, tick);
criterion_main!(benches);
