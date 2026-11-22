//! `anim.sample` under simulation LOD (M2 phase 2.4b.1, ADR-0076): far characters pose at their
//! LOD rate, advancing by their elapsed time, so each event fires once on the right tick and the
//! pose at a due tick equals a full-rate character's. 8 Hz ticks (1/8 s); a 1 s looping clip moves
//! `hip` from x = 0 to 1, with events `left` at 0.25 and `right` at 0.3.
//!
//! Stagger phases (computed separately, as in `concordia-terra`'s `tests/lod.rs`): `…a0` and `…b0`
//! ≡ 3 (mod 4), `…c0` ≡ 2, `…d0` ≡ 1. At level 2 (rate 4) `…b0` is due on ticks 0, 1, 5, 9, 13
//! and `…c0` on 0, 2, 6, 10, 14.

mod common;

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use common::{clip, joint};
use concordia_anim::{
    AnimEvent, AnimGraphPlayer, AnimLibrary, AnimPlayer, BoundClip, BoundGraph, Pose, Skeleton,
    add_processors, insert_rest_poses,
};
use concordia_assets::{
    AnimGraph, AnimNode, AssetRef, ChannelPath, ClipEvent, Interpolation, Skinned,
};
use concordia_core::id::EntityId;
use concordia_core::jobs::JobSystem;
use concordia_core::math::Vec3;
use concordia_core::time::TickRate;
use concordia_scene::builtin::{Transform, lod_position};
use concordia_terra::{LodProfile, LodViewer, Phase, Schedule, SimLoop, World, lod_system};

const FILE: &str = "018bcfe5-6800-7000-a424-6c55076b137f";

fn id(tail: &str) -> EntityId {
    format!("018bcfe5-6800-7000-a424-0000000000{tail}")
        .parse()
        .unwrap()
}

fn skin() -> AssetRef {
    format!("asset:{FILE}#skin/0").parse().unwrap()
}

fn steps() -> AssetRef {
    format!("asset:{FILE}#clip/steps").parse().unwrap()
}

fn graph_ref() -> AssetRef {
    format!("asset:{FILE}#graph/0").parse().unwrap()
}

fn library() -> AnimLibrary {
    let skeleton = Skeleton::new(vec![
        joint("hip", None, Vec3::ZERO),
        joint("arm", Some(0), Vec3::Y),
    ])
    .unwrap();
    let mut data = clip(
        "hip",
        ChannelPath::Translation,
        Interpolation::Linear,
        &[0.0, 1.0],
        &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    );
    data.name = "steps".into();
    data.events = ["left", "right"]
        .iter()
        .zip([0.25, 0.3])
        .map(|(name, time)| ClipEvent {
            time,
            name: (*name).to_owned(),
        })
        .collect();
    let graph = AnimGraph {
        parameters: BTreeMap::new(),
        nodes: [(
            "walk".to_owned(),
            AnimNode::Clip {
                clip: steps(),
                looping: true,
            },
        )]
        .into_iter()
        .collect(),
        root: "walk".to_owned(),
    };
    let bound = BoundClip::bind(&data, &skeleton).unwrap();
    let graph = BoundGraph::bind(&graph, &skeleton, &mut |_| {
        BoundClip::bind(&data, &skeleton)
    })
    .unwrap();
    let mut library = AnimLibrary::new();
    library.insert_clip(steps(), skin(), bound);
    library.insert_graph(graph_ref(), skin(), graph);
    library.insert_skeleton(skin(), skeleton);
    library
}

fn at(world: &mut World, id: EntityId, x: f32) {
    world.spawn_with_id(id).unwrap();
    let mut t = Transform::default();
    t.translation.x = x;
    world.insert(id, t).unwrap();
}

fn character(world: &mut World, id: EntityId, x: f32, graph: bool) {
    at(world, id, x);
    world.insert(id, Skinned { skin: skin() }).unwrap();
    world
        .insert(
            id,
            LodProfile::new([10.0, 20.0, 40.0], [2, 4, 8], 1.0).unwrap(),
        )
        .unwrap();
    if graph {
        world.insert(id, AnimGraphPlayer::new(graph_ref())).unwrap();
    } else {
        world.insert(id, AnimPlayer::new(steps())).unwrap();
    }
}

fn hip(world: &World, id: EntityId) -> f32 {
    world.get::<Pose>(id).unwrap().bones[0].translation.x
}

#[test]
fn far_characters_fire_events_once_on_the_right_tick_and_match_full_rate_poses() {
    let (viewer, near, far, far_graph, near_graph) =
        (id("e0"), id("a0"), id("b0"), id("c0"), id("d0"));
    let library = Arc::new(library());
    let mut world = World::new();
    world.register_shared::<LodProfile>().unwrap();
    at(&mut world, viewer, 0.0);
    world.insert(viewer, LodViewer).unwrap();
    character(&mut world, near, 5.0, false);
    character(&mut world, far, 30.0, false);
    character(&mut world, far_graph, 30.0, true);
    character(&mut world, near_graph, 5.0, true);
    insert_rest_poses(&library, &mut world);
    let mut schedule = Schedule::new();
    schedule.add_in(Phase::PrePhysics, lod_system(lod_position));
    add_processors(&mut schedule, library);
    let mut sim = SimLoop::new(TickRate::from_hz(8).unwrap(), schedule, &mut world).unwrap();
    let jobs = JobSystem::new(NonZeroUsize::new(2).unwrap()).unwrap();

    let mut raised: Vec<(u64, EntityId, String)> = Vec::new();
    let mut held = (0.0f32, 0.0f32);
    for tick in 0..16u64 {
        sim.step(&mut world, &jobs, 1);
        for (target, event) in world.pending_signals::<AnimEvent>() {
            raised.push((tick, target, event.name.clone()));
        }
        // Full rate: after tick t the clip is at (t + 1) / 8, wrapped.
        let full = ((tick + 1) as f32 * 0.125).rem_euclid(1.0);
        assert!((hip(&world, near) - full).abs() < 1e-6, "tick {tick}");
        assert!((hip(&world, near_graph) - full).abs() < 1e-6, "tick {tick}");
        if [0, 1, 5, 9, 13].contains(&tick) {
            assert!(
                (hip(&world, far) - full).abs() < 1e-6,
                "far at its due tick {tick}"
            );
        } else {
            assert_eq!(
                hip(&world, far),
                held.0,
                "far holds between due ticks ({tick})"
            );
        }
        if [0, 2, 6, 10, 14].contains(&tick) {
            let got = hip(&world, far_graph);
            assert!(
                (got - full).abs() < 1e-6,
                "far graph at its due tick {tick}"
            );
        } else {
            assert_eq!(hip(&world, far_graph), held.1, "far graph holds ({tick})");
        }
        held = (hip(&world, far), hip(&world, far_graph));
    }
    let of = |who: EntityId| -> Vec<(u64, String)> {
        raised
            .iter()
            .filter(|r| r.1 == who)
            .map(|r| (r.0, r.2.clone()))
            .collect()
    };
    let pair = |t: u64| [(t, "left".to_owned()), (t, "right".to_owned())];
    let twice = |a: u64, b: u64| [pair(a), pair(b)].concat();
    // Full rate: [0.25, 0.375) is tick 2, and again on the next lap (tick 10).
    assert_eq!(of(near), twice(2, 10));
    assert_eq!(of(near_graph), twice(2, 10));
    // Far clip: steps [.25, .75) on ticks 5 and 13; [.75, 1.25) on tick 9 crosses nothing.
    assert_eq!(of(far), twice(5, 13));
    // Far graph: [.125, .375) on tick 2, [.875, 1.375) on tick 10.
    assert_eq!(of(far_graph), twice(2, 10));
}
