//! The play processors in `FrameEnd`: `anim.pose` gives new players a rest pose, and
//! `anim.sample` advances each player by the tick's scaled dt and samples its clip into its pose.

use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_assets::Skinned;
use concordia_core::id::EntityId;
use concordia_terra::{Phase, Schedule, SystemCtx, View, World, processor};

use crate::library::AnimLibrary;
use crate::player::{AnimPlayer, advance};
use crate::pose::Pose;

/// Entities per parallel piece (fixed, so results never depend on the thread count).
pub const CHUNK: NonZeroUsize = NonZeroUsize::new(64).unwrap();

/// `anim.pose`: gives players a rest pose.
pub const POSE: &str = "anim.pose";
/// `anim.sample`: advances players and samples their clips.
pub const SAMPLE: &str = "anim.sample";

/// Gives every `AnimPlayer` entity with a known skeleton its rest `Pose`, if it has none.
pub fn insert_rest_poses(library: &AnimLibrary, world: &mut World) {
    let mut missing = Vec::new();
    if let Ok(mut query) = world.query::<(&AnimPlayer, &Skinned)>() {
        query.for_each(world, |id, (_, skinned)| {
            if let Some(skeleton) = library.skeleton(&skinned.skin) {
                missing.push((id, skeleton.rest_pose()));
            }
        });
    }
    for (id, pose) in missing {
        if world.get::<Pose>(id).is_none() {
            let inserted = world.insert(id, pose);
            debug_assert!(inserted.is_ok(), "the entity was just queried");
        }
    }
}

/// Adds `anim.pose` and then `anim.sample` to `FrameEnd`.
pub fn add_processors(schedule: &mut Schedule, library: Arc<AnimLibrary>) {
    let rest = library.clone();
    schedule.add_processor(
        processor(POSE, Phase::FrameEnd)
            .reads::<AnimPlayer>()
            .reads::<Skinned>()
            .reads::<Pose>()
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| give_rest_poses(&rest, view)),
    );
    schedule.add_processor(
        processor(SAMPLE, Phase::FrameEnd)
            .writes::<AnimPlayer>()
            .writes::<Pose>()
            .reads::<Skinned>()
            .after(POSE)
            .run(move |view: &mut View<'_>, ctx: &SystemCtx<'_>| sample_all(&library, view, ctx)),
    );
}

/// Queues a rest pose for each player that has a known skeleton and no pose yet.
fn give_rest_poses(library: &AnimLibrary, view: &mut View<'_>) {
    let mut posed = BTreeSet::new();
    if let Ok(query) = view.query::<&Pose>() {
        query.for_each(|id, _| {
            posed.insert(id);
        });
    }
    let mut new: Vec<(EntityId, Pose)> = Vec::new();
    if let Ok(query) = view.query::<(&AnimPlayer, &Skinned)>() {
        query.for_each(|id, (_, skinned)| {
            let skeleton = library.skeleton(&skinned.skin);
            if let (false, Some(skeleton)) = (posed.contains(&id), skeleton) {
                new.push((id, skeleton.rest_pose()));
            }
        });
    }
    if let Ok(commands) = view.commands() {
        for (id, pose) in new {
            commands.insert(id, pose);
        }
    }
}

/// Advances every posed player by the scaled dt and samples its clip, in parallel.
fn sample_all(library: &AnimLibrary, view: &mut View<'_>, ctx: &SystemCtx<'_>) {
    let dt = ctx.dt;
    if let Ok(query) = view.query::<(&mut AnimPlayer, &Skinned, &mut Pose)>() {
        query.par_for_each(ctx.jobs, CHUNK, |_, (player, skinned, pose)| {
            sample_one(library, dt, player, skinned, pose);
        });
    }
}

/// One player: unknown skeletons or clips hold the pose (the library reported them at play start).
fn sample_one(
    library: &AnimLibrary,
    dt: f32,
    player: &mut AnimPlayer,
    skinned: &Skinned,
    pose: &mut Pose,
) {
    let (Some(skeleton), Some(clip)) = (
        library.skeleton(&skinned.skin),
        library.clip(&player.clip, &skinned.skin),
    ) else {
        return;
    };
    player.time = advance(
        player.time,
        dt * player.speed,
        clip.duration(),
        player.looping,
    );
    skeleton.reset(pose);
    let sampled = clip.sample(player.time, pose);
    debug_assert!(sampled.is_ok(), "the rest pose fits its skeleton");
}
