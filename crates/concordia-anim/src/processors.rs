//! The play processors in `FrameEnd`: `anim.pose` gives new players a rest pose (and graph
//! players their starting state), and `anim.sample` advances each player by the tick's scaled dt:
//! graph players evaluate their graph, clip players sample their clip, into the entity's pose,
//! and the clip events they cross are raised as `AnimEvent` signals (2.4.4).

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_assets::Skinned;
use concordia_core::id::EntityId;
use concordia_terra::{Phase, Schedule, SystemCtx, View, World, processor};

use crate::events::{AnimEvent, crossed_events};
use crate::graph_player::{AnimGraphPlayer, AnimGraphState};
use crate::library::AnimLibrary;
use crate::player::{AnimPlayer, advance};
use crate::pose::Pose;

/// Entities per parallel piece (fixed, so results never depend on the thread count).
pub const CHUNK: NonZeroUsize = NonZeroUsize::new(64).unwrap();

/// `anim.pose`: gives players a rest pose.
pub const POSE: &str = "anim.pose";
/// `anim.sample`: advances players, evaluating graphs and sampling clips.
pub const SAMPLE: &str = "anim.sample";

/// Gives every player entity with a known skeleton its rest `Pose`, and every graph player with a
/// known graph its starting `AnimGraphState`, where missing.
pub fn insert_rest_poses(library: &AnimLibrary, world: &mut World) {
    let mut poses = Vec::new();
    let mut states = Vec::new();
    if let Ok(mut query) = world.query::<(&AnimPlayer, &Skinned)>() {
        query.for_each(world, |id, (_, skinned)| {
            if let Some(skeleton) = library.skeleton(&skinned.skin) {
                poses.push((id, skeleton.rest_pose()));
            }
        });
    }
    if let Ok(mut query) = world.query::<(&AnimGraphPlayer, &Skinned)>() {
        query.for_each(world, |id, (player, skinned)| {
            if let Some(skeleton) = library.skeleton(&skinned.skin) {
                poses.push((id, skeleton.rest_pose()));
            }
            if let Some(graph) = library.graph(&player.graph, &skinned.skin) {
                states.push((id, graph.initial_state()));
            }
        });
    }
    for (id, pose) in poses {
        if world.get::<Pose>(id).is_none() {
            let inserted = world.insert(id, pose);
            debug_assert!(inserted.is_ok(), "the entity was just queried");
        }
    }
    for (id, state) in states {
        if world.get::<AnimGraphState>(id).is_none() {
            let inserted = world.insert(id, state);
            debug_assert!(inserted.is_ok(), "the entity was just queried");
        }
    }
}

/// Adds `anim.pose`, `anim.sample` and then `anim.attach` to `FrameEnd`.
pub fn add_processors(schedule: &mut Schedule, library: Arc<AnimLibrary>) {
    let rest = library.clone();
    schedule.add_processor(
        processor(POSE, Phase::FrameEnd)
            .reads::<AnimPlayer>()
            .reads::<AnimGraphPlayer>()
            .reads::<AnimGraphState>()
            .reads::<Skinned>()
            .reads::<Pose>()
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| give_rest_poses(&rest, view)),
    );
    schedule.add_processor(
        processor(SAMPLE, Phase::FrameEnd)
            .writes::<AnimPlayer>()
            .writes::<AnimGraphPlayer>()
            .writes::<AnimGraphState>()
            .writes::<Pose>()
            .reads::<Skinned>()
            .after(POSE)
            .run({
                let library = library.clone();
                move |view: &mut View<'_>, ctx: &SystemCtx<'_>| sample_all(&library, view, ctx)
            }),
    );
    crate::attach::add_attach(schedule, library);
}

/// Queues a rest pose for each player that has a known skeleton and no pose yet, and a starting
/// state for each graph player that has a known graph and none.
fn give_rest_poses(library: &AnimLibrary, view: &mut View<'_>) {
    let posed = ids_with::<Pose>(view);
    let stated = ids_with::<AnimGraphState>(view);
    let mut poses: BTreeMap<EntityId, Pose> = BTreeMap::new();
    let mut states: Vec<(EntityId, AnimGraphState)> = Vec::new();
    let mut want_pose = |id: EntityId, skinned: &Skinned| {
        let skeleton = library.skeleton(&skinned.skin);
        if let (false, Some(skeleton)) = (posed.contains(&id), skeleton) {
            poses.insert(id, skeleton.rest_pose());
        }
    };
    if let Ok(query) = view.query::<(&AnimPlayer, &Skinned)>() {
        query.for_each(|id, (_, skinned)| want_pose(id, skinned));
    }
    if let Ok(query) = view.query::<(&AnimGraphPlayer, &Skinned)>() {
        query.for_each(|id, (player, skinned)| {
            want_pose(id, skinned);
            let graph = library.graph(&player.graph, &skinned.skin);
            if let (false, Some(graph)) = (stated.contains(&id), graph) {
                states.push((id, graph.initial_state()));
            }
        });
    }
    if let Ok(commands) = view.commands() {
        for (id, pose) in poses {
            commands.insert(id, pose);
        }
        for (id, state) in states {
            commands.insert(id, state);
        }
    }
}

/// Entities in the view that have `T`, sorted.
fn ids_with<T: concordia_terra::Component>(view: &mut View<'_>) -> BTreeSet<EntityId> {
    let mut ids = BTreeSet::new();
    if let Ok(query) = view.query::<&T>() {
        query.for_each(|id, _| {
            ids.insert(id);
        });
    }
    ids
}

/// Advances every posed player by the scaled dt, in parallel: graph players evaluate their graph,
/// then clip players without a graph sample their clip.
fn sample_all(library: &AnimLibrary, view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let dt = ctx.dt;
    if let Ok(query) = view.query::<(
        &mut AnimGraphPlayer,
        &mut AnimGraphState,
        &Skinned,
        &mut Pose,
    )>() {
        query.par_for_each_deferred(
            ctx.jobs,
            CHUNK,
            |commands, id, (player, state, skinned, pose)| {
                for event in evaluate_one(library, dt, player, state, skinned, pose) {
                    commands.signal(id, event);
                }
            },
        );
    }
    let graphs = ids_with::<AnimGraphPlayer>(view);
    if let Ok(query) = view.query::<(&mut AnimPlayer, &Skinned, &mut Pose)>() {
        query.par_for_each_deferred(ctx.jobs, CHUNK, |commands, id, (player, skinned, pose)| {
            if !graphs.contains(&id) {
                for event in sample_one(library, dt, player, skinned, pose) {
                    commands.signal(id, event);
                }
            }
        });
    }
}

/// One graph player, returning the events it crossed: unknown skeletons or graphs hold the pose
/// (reported at play start).
fn evaluate_one(
    library: &AnimLibrary,
    dt: f32,
    player: &mut AnimGraphPlayer,
    state: &mut AnimGraphState,
    skinned: &Skinned,
    pose: &mut Pose,
) -> Vec<AnimEvent> {
    let (Some(skeleton), Some(graph)) = (
        library.skeleton(&skinned.skin),
        library.graph(&player.graph, &skinned.skin),
    ) else {
        return Vec::new();
    };
    let evaluated = graph.evaluate(dt, player, state, skeleton, pose);
    debug_assert!(evaluated.is_ok(), "graphs bound to this skeleton");
    evaluated.unwrap_or_default()
}

/// One player, returning the events it crossed: unknown skeletons or clips hold the pose (the
/// library reported them at play start).
fn sample_one(
    library: &AnimLibrary,
    dt: f32,
    player: &mut AnimPlayer,
    skinned: &Skinned,
    pose: &mut Pose,
) -> Vec<AnimEvent> {
    let (Some(skeleton), Some(clip)) = (
        library.skeleton(&skinned.skin),
        library.clip(&player.clip, &skinned.skin),
    ) else {
        return Vec::new();
    };
    let delta = dt * player.speed;
    let events = crossed_events(
        clip.events(),
        clip.duration(),
        player.looping,
        player.time,
        delta,
    )
    .into_iter()
    .filter_map(|c| clip.events().get(c.event))
    .map(|e| AnimEvent {
        name: e.name.clone(),
        clip: player.clip.clone(),
        node: None,
        time: e.time,
        weight: 1.0,
    })
    .collect();
    player.time = advance(player.time, delta, clip.duration(), player.looping);
    skeleton.reset(pose);
    let sampled = clip.sample(player.time, pose);
    debug_assert!(sampled.is_ok(), "the rest pose fits its skeleton");
    events
}
