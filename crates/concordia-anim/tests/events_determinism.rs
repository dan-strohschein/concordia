//! Events, attachments and tweens hash the same at 1, 3 and 8 threads (2.4.4), and the run isn't
//! vacuous: events are delivered, attachments move and tweens run.

mod common;

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use common::{clip, joint};
use concordia_anim::{
    AnimEvent, AnimGraphPlayer, AnimLibrary, AnimPlayer, BoneAttachment, BoundClip, BoundGraph,
    Ease, PropertyTween, Skeleton, Transition, Tweens, add_processors, add_tweens,
    insert_rest_poses,
};
use concordia_assets::{
    AnimGraph, AnimNode, AssetRef, ChannelPath, ClipEvent, Interpolation, Parameter, Skinned,
};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::math::Vec3;
use concordia_core::time::TickRate;
use concordia_reflect::TypeRegistry;
use concordia_scene::builtin::{Parent, Transform};
use concordia_terra::{Phase, Schedule, SimLoop, SystemCtx, View, World, WorldHash, processor};

const FILE: &str = "018bcfe5-6800-7000-a424-6c55076b137f";
const CHARACTERS: usize = 100;

fn r(sub: &str) -> AssetRef {
    format!("asset:{FILE}{sub}").parse().unwrap()
}

fn skeleton() -> Skeleton {
    Skeleton::new(vec![
        joint("hip", None, Vec3::ZERO),
        joint("arm", Some(0), Vec3::Y),
    ])
    .unwrap()
}

/// A 0.7 s clip swinging the arm, with three events.
fn bound(name: &str, skeleton: &Skeleton) -> BoundClip {
    let mut data = clip(
        "arm",
        ChannelPath::Rotation,
        Interpolation::Linear,
        &[0.0, 0.35, 0.7],
        &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.6, 0.8, 0.0, 0.0, 0.0, 1.0],
    );
    data.name = name.into();
    data.duration = 0.7;
    data.events = [(0.0, "lift"), (0.2, "swing"), (0.55, "land")]
        .into_iter()
        .map(|(time, name)| ClipEvent {
            time,
            name: name.into(),
        })
        .collect();
    BoundClip::bind(&data, skeleton).unwrap()
}

fn library() -> AnimLibrary {
    let skeleton = skeleton();
    let clip = bound("swing", &skeleton);
    let graph = AnimGraph {
        parameters: [("amount".to_owned(), Parameter::Float(0.4))]
            .into_iter()
            .collect(),
        nodes: [
            (
                "a",
                AnimNode::Clip {
                    clip: r("#clip/swing"),
                    looping: true,
                },
            ),
            (
                "b",
                AnimNode::Clip {
                    clip: r("#clip/swing"),
                    looping: false,
                },
            ),
            (
                "mix",
                AnimNode::Blend2 {
                    a: "a".into(),
                    b: "b".into(),
                    amount: "amount".into(),
                    filter: vec![],
                },
            ),
        ]
        .into_iter()
        .map(|(n, d)| (n.to_owned(), d))
        .collect(),
        root: "mix".into(),
    };
    let bound_graph = BoundGraph::bind(&graph, &skeleton, &mut |_| Ok(clip.clone())).unwrap();
    let mut library = AnimLibrary::new();
    library.insert_clip(r("#clip/swing"), r("#skin/0"), clip);
    library.insert_graph(r(""), r("#skin/0"), bound_graph);
    library.insert_skeleton(r("#skin/0"), skeleton);
    library
}

fn registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    concordia_assets::register_types(&mut registry).unwrap();
    concordia_anim::register_types(&mut registry).unwrap();
    registry
}

/// Characters with clip or graph players at mixed speeds and offsets, each holding an attached
/// item, every third one with a tween.
fn world(library: &AnimLibrary) -> (World, Vec<EntityId>) {
    let mut world = World::new();
    let mut ids = DeterministicIdGen::new(11);
    let mut items = Vec::new();
    for i in 0..CHARACTERS {
        let c = world.spawn(&mut ids).unwrap();
        world.insert(c, Transform::default()).unwrap();
        world.insert(c, Skinned { skin: r("#skin/0") }).unwrap();
        if i % 2 == 0 {
            let mut p = AnimPlayer::new(r("#clip/swing"));
            p.time = (i % 7) as f32 * 0.1;
            p.speed = [1.0, -0.5, 2.5][i % 3];
            p.looping = i % 5 != 0;
            world.insert(c, p).unwrap();
        } else {
            let mut p = AnimGraphPlayer::new(r(""));
            p.floats.insert("amount".into(), (i % 4) as f32 / 3.0);
            world.insert(c, p).unwrap();
        }
        if i % 3 == 0 {
            let mut t = PropertyTween {
                component: "Transform".into(),
                field: "/translation".into(),
                to: vec![i as f32, 1.0, 0.0],
                from: None,
                duration: 2.0 + (i % 4) as f32,
                delay: (i % 2) as f32 * 0.3,
                elapsed: 0.0,
                transition: Transition::Elastic,
                ease: Ease::Out,
            };
            if i % 9 == 0 {
                t.transition = Transition::Bounce;
            }
            world.insert(c, Tweens { tweens: vec![t] }).unwrap();
        }
        let item = world.spawn(&mut ids).unwrap();
        world.insert(item, Parent(c)).unwrap();
        world.insert(item, Transform::default()).unwrap();
        world
            .insert(item, BoneAttachment { bone: "arm".into() })
            .unwrap();
        items.push(item);
    }
    insert_rest_poses(library, &mut world);
    (world, items)
}

struct Outcome {
    hashes: Vec<WorldHash>,
    delivered: usize,
    moved: usize,
}

fn run(threads: usize) -> Outcome {
    let library = Arc::new(library());
    let (mut world, items) = world(&library);
    let start: Vec<Transform> = items
        .iter()
        .map(|i| *world.get::<Transform>(*i).unwrap())
        .collect();
    let delivered = Arc::new(Mutex::new(0usize));
    let mut schedule = Schedule::new();
    let counter = delivered.clone();
    schedule.add_processor(
        processor("test.count", Phase::PrePhysics)
            .receives::<AnimEvent>()
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                *counter.lock().unwrap() += view.signals::<AnimEvent>().unwrap().len();
            }),
    );
    add_tweens(&mut schedule, &registry());
    add_processors(&mut schedule, library);
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    let jobs = JobSystem::new(NonZeroUsize::new(threads).unwrap()).unwrap();
    let mut hashes = Vec::new();
    let mut done = 0;
    for tick in [1, 60, 300] {
        sim.step(&mut world, &jobs, tick - done);
        done = tick;
        hashes.push(world.state_hash().unwrap());
    }
    let moved = items
        .iter()
        .zip(&start)
        .filter(|(i, s)| world.get::<Transform>(**i).unwrap() != *s)
        .count();
    let delivered = *delivered.lock().unwrap();
    Outcome {
        hashes,
        delivered,
        moved,
    }
}

#[test]
fn events_and_attachments_are_deterministic_across_thread_counts() {
    let one = run(1);
    assert!(
        one.delivered > 500,
        "events are delivered: {}",
        one.delivered
    );
    assert_eq!(one.moved, CHARACTERS, "every attached item moved");
    for threads in [3, 8] {
        let other = run(threads);
        assert_eq!(one.hashes, other.hashes, "{threads} threads");
        assert_eq!(one.delivered, other.delivered, "{threads} threads");
    }
}
