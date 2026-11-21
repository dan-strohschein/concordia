//! Sampling throughput (`just bench -p concordia-anim`); results recorded in `BASELINE.md`.
#![allow(missing_docs)]

use std::hint::black_box;
use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_anim::{
    AnimGraphPlayer, AnimLibrary, AnimPlayer, BoundClip, BoundGraph, Joint, Skeleton,
    add_processors,
};
use concordia_assets::{
    AdvanceMode, AnimGraph, AnimNode, AssetRef, Channel, ChannelPath, ClipData, FadeCurve,
    Interpolation, Parameter, Skinned, StateTransition, SwitchMode,
};
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

/// Idle and walk states: walk is a `Blend2` of two clips under a one-shot of a third.
fn locomotion(clip: &AssetRef) -> AnimGraph {
    let clip_node = |looping| AnimNode::Clip {
        clip: clip.clone(),
        looping,
    };
    let nodes = [
        ("idle", clip_node(true)),
        ("slow", clip_node(true)),
        ("fast", clip_node(true)),
        ("swing", clip_node(false)),
        (
            "gait",
            AnimNode::Blend2 {
                a: "slow".into(),
                b: "fast".into(),
                amount: "speed".into(),
                filter: vec![],
            },
        ),
        (
            "walk",
            AnimNode::OneShot {
                base: "gait".into(),
                shot: "swing".into(),
                request: "attack".into(),
                fade_in: 0.1,
                fade_out: 0.2,
                curve: FadeCurve::Linear,
                filter: (0..BONES / 2).map(|j| format!("bone{j}")).collect(),
            },
        ),
        (
            "moves",
            AnimNode::StateMachine {
                states: [("idle", "idle"), ("walk", "walk")]
                    .into_iter()
                    .map(|(s, n)| (s.to_owned(), n.to_owned()))
                    .collect(),
                start: "walk".into(),
                transitions: vec![
                    StateTransition {
                        from: "walk".into(),
                        to: "idle".into(),
                        switch: SwitchMode::Immediate,
                        advance: AdvanceMode::Enabled,
                        condition: None,
                        xfade: 0.2,
                        curve: FadeCurve::Linear,
                        reset: true,
                        priority: 1,
                    },
                    StateTransition {
                        from: "idle".into(),
                        to: "walk".into(),
                        switch: SwitchMode::Immediate,
                        advance: AdvanceMode::Enabled,
                        condition: None,
                        xfade: 0.2,
                        curve: FadeCurve::Linear,
                        reset: true,
                        priority: 1,
                    },
                ],
            },
        ),
    ];
    AnimGraph {
        parameters: [
            ("speed".to_owned(), Parameter::Float(0.5)),
            ("attack".to_owned(), Parameter::Trigger),
        ]
        .into_iter()
        .collect(),
        nodes: nodes.into_iter().map(|(n, d)| (n.to_owned(), d)).collect(),
        root: "moves".into(),
    }
}

/// 1,000 characters through `locomotion`, each with an active one-shot (three clips blended).
fn graph(c: &mut Criterion) {
    let skin: AssetRef = "asset:018bcfe5-6800-7000-a424-6c55076b137f#skin/0"
        .parse()
        .unwrap();
    let clip_ref: AssetRef = "asset:018bcfe5-6800-7000-a424-6c55076b137f#clip/bench"
        .parse()
        .unwrap();
    let graph_ref: AssetRef = "asset:018bcfe5-6800-7000-a424-6c55076b137f"
        .parse()
        .unwrap();
    let skeleton = skeleton();
    let bound = BoundClip::bind(&clip(), &skeleton).unwrap();
    let graph = BoundGraph::bind(
        &locomotion(&clip_ref),
        &skeleton,
        &mut |_| Ok(bound.clone()),
    )
    .unwrap();
    let mut library = AnimLibrary::new();
    library.insert_graph(graph_ref.clone(), skin.clone(), graph);
    library.insert_skeleton(skin.clone(), skeleton);
    let library = Arc::new(library);
    let mut world = World::new();
    let mut ids = DeterministicIdGen::new(3);
    for i in 0..CHARACTERS {
        let id = world.spawn(&mut ids).unwrap();
        world.insert(id, Skinned { skin: skin.clone() }).unwrap();
        let mut player = AnimGraphPlayer::new(graph_ref.clone());
        player.floats.insert("speed".into(), (i % 10) as f32 / 10.0);
        world.insert(id, player).unwrap();
    }
    concordia_anim::insert_rest_poses(&library, &mut world);
    let mut schedule = Schedule::new();
    add_processors(&mut schedule, library);
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    for threads in [1, 8] {
        let jobs = JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap();
        c.bench_function(&format!("graph_1000_characters_{threads}_threads"), |b| {
            b.iter(|| {
                // Keep every one-shot playing: fire it again before it ends.
                let mut q = world.query::<&mut AnimGraphPlayer>().unwrap();
                q.for_each(&mut world, |_, p| p.triggers.push("attack".into()));
                sim.step(&mut world, &jobs, 1);
            });
        });
    }
}

criterion_group!(benches, sample, tick, graph);
criterion_main!(benches);
