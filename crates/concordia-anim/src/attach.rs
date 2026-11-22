//! `BoneAttachment` (M2 phase 2.4.4, Godot's `BoneAttachment3D`): an entity whose parent is a
//! posed character follows one of its bones; items are children of the attachment entity.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use concordia_assets::Skinned;
use concordia_core::id::EntityId;
use concordia_core::math::Mat4;
use concordia_reflect::Reflect;
use concordia_scene::builtin::{Parent, Transform};
use concordia_terra::{Phase, Schedule, SystemCtx, View, processor};
use serde::{Deserialize, Serialize};

use crate::library::AnimLibrary;
use crate::pose::Pose;
use crate::processors::SAMPLE;

/// `anim.attach`: moves bone attachments to their bones, after `anim.sample`.
pub const ATTACH: &str = "anim.attach";

/// Follows bone `bone` of the posed character that is this entity's `Parent`: its `Transform`
/// becomes the bone's transform in the character's space every tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Animation")]
#[serde(deny_unknown_fields)]
pub struct BoneAttachment {
    /// The bone's name (a joint of the parent's skeleton).
    pub bone: String,
}

/// Adds `anim.attach` to `FrameEnd`, after `anim.sample`.
pub(crate) fn add_attach(schedule: &mut Schedule, library: Arc<AnimLibrary>) {
    let warned = Mutex::new(BTreeSet::new());
    schedule.add_processor(
        processor(ATTACH, Phase::FrameEnd)
            .reads::<BoneAttachment>()
            .reads::<Parent>()
            .reads::<Pose>()
            .reads::<Skinned>()
            .writes::<Transform>()
            .after(SAMPLE)
            .run(move |view: &mut View<'_>, _: &SystemCtx<'_>| {
                attach_all(&library, &warned, view);
            }),
    );
}

/// Every attachment's bone transform, written to its `Transform`; ones that can't be placed keep
/// theirs and are logged once each.
fn attach_all(library: &AnimLibrary, warned: &Mutex<BTreeSet<EntityId>>, view: &mut View<'_>) {
    let wanted = attachments(view);
    let parents: Vec<EntityId> = wanted.keys().copied().collect();
    let mut placed: BTreeMap<EntityId, Transform> = BTreeMap::new();
    if let Ok(query) = view.query::<(&Pose, &Skinned)>() {
        query.for_each_of(&parents, |parent, (pose, skinned)| {
            let Some(skeleton) = library.skeleton(&skinned.skin) else {
                return;
            };
            for (id, bone) in &wanted[&parent] {
                let matrix = skeleton
                    .index_of(bone)
                    .and_then(|j| skeleton.entity_matrix(pose, j));
                if let Some(m) = matrix {
                    placed.insert(*id, transform_of(m));
                }
            }
        });
    }
    let ids: Vec<EntityId> = placed.keys().copied().collect();
    if let Ok(query) = view.query::<&mut Transform>() {
        query.for_each_of(&ids, |id, t| *t = placed[&id]);
    }
    warn_unplaced(&wanted, &placed, warned);
}

/// Attachments grouped by parent (sorted), each with its bone name.
fn attachments(view: &mut View<'_>) -> BTreeMap<EntityId, Vec<(EntityId, String)>> {
    let mut by_parent: BTreeMap<EntityId, Vec<(EntityId, String)>> = BTreeMap::new();
    if let Ok(query) = view.query::<(&BoneAttachment, &Parent)>() {
        query.for_each(|id, (a, p)| by_parent.entry(p.0).or_default().push((id, a.bone.clone())));
    }
    by_parent
}

/// Logs each attachment that couldn't be placed (no such bone, or a parent without a pose), once.
fn warn_unplaced(
    wanted: &BTreeMap<EntityId, Vec<(EntityId, String)>>,
    placed: &BTreeMap<EntityId, Transform>,
    warned: &Mutex<BTreeSet<EntityId>>,
) {
    let Ok(mut warned) = warned.lock() else {
        return;
    };
    for (parent, list) in wanted {
        for (id, bone) in list.iter().filter(|(id, _)| !placed.contains_key(id)) {
            if warned.insert(*id) {
                tracing::warn!(target: "concordia::anim", entity = %id, %parent, bone, "bone attachment not placed: its parent has no pose, or no such bone");
            }
        }
    }
}

/// A matrix as a transform (rotation normalized).
fn transform_of(m: Mat4) -> Transform {
    let (scale, rotation, translation) = m.to_scale_rotation_translation();
    Transform {
        translation,
        rotation: rotation.normalize(),
        scale,
    }
}
