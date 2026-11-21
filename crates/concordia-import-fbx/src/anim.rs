//! Animation stacks → clips (`clip/<name>`): baked linear keys from time 0 at the file's frame rate.

use concordia_assets::{AssetKind, Channel, ChannelPath, ClipData, ImportOutput, Interpolation};
use concordia_core::id::EntityId;
use concordia_import_model::insert_artifact;

use crate::ffi::{Bake, Key, Scene, Stack};
use crate::names::Names;

/// Keys per second when the file declares no frame rate.
const DEFAULT_RATE: f64 = 30.0;

const PATHS: [(ChannelPath, usize); 3] = [
    (ChannelPath::Translation, 3),
    (ChannelPath::Rotation, 4),
    (ChannelPath::Scale, 3),
];

/// Imports every stack as a clip whose channels target prefab entities.
pub(crate) fn clips(
    scene: &Scene<'_>,
    stacks: &[Stack],
    names: &Names,
    entities: &[Option<EntityId>],
    out: &mut ImportOutput,
) -> Result<(), String> {
    let fps = scene.frames_per_second();
    let rate = if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        DEFAULT_RATE
    };
    for (index, _) in stacks.iter().enumerate() {
        let sub = format!("clip/{}", names.clips[index]);
        let bake = scene.bake(index, rate).map_err(|e| format!("{sub}: {e}"))?;
        let data = ClipData {
            name: names.clips[index].clone(),
            duration: bake.duration() as f32,
            channels: channels(&bake, entities),
        };
        insert_artifact(out, sub, AssetKind::Clip, &data).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Every keyed channel of every baked node that is a prefab entity.
fn channels(bake: &Bake<'_>, entities: &[Option<EntityId>]) -> Vec<Channel> {
    let mut out = Vec::new();
    for node in bake.nodes() {
        let Some(target) = entities.get(node.node).copied().flatten() else {
            continue;
        };
        for (channel, (path, width)) in PATHS.into_iter().enumerate() {
            let keys = bake.keys(&node, channel);
            if !keys.is_empty() {
                out.push(channel_of(target, path, width, &keys));
            }
        }
    }
    out
}

fn channel_of(target: EntityId, path: ChannelPath, width: usize, keys: &[Key]) -> Channel {
    Channel {
        target,
        path,
        interpolation: Interpolation::Linear,
        times: keys.iter().map(|k| k.time).collect(),
        values: keys
            .iter()
            .flat_map(|k| k.value[..width].to_vec())
            .collect(),
    }
}
