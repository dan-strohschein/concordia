//! Skins (`skin/<index>`) and animation clips (`clip/<name>`), stored for M2's animation system.

use concordia_assets::{
    Artifact, AssetKind, Channel, ChannelPath, ClipData, ImportOutput, Interpolation, SkinData,
    encode,
};
use concordia_core::id::EntityId;

use crate::names::Names;

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

pub(crate) fn skins_and_clips(
    doc: &gltf::Document,
    buffers: &[Vec<u8>],
    names: &Names,
    entities: &[EntityId],
    out: &mut ImportOutput,
) -> Result<(), String> {
    let get = |b: gltf::Buffer<'_>| buffers.get(b.index()).map(Vec::as_slice);
    for skin in doc.skins() {
        let joints: Vec<gltf::Node<'_>> = skin.joints().collect();
        let inverse_bind: Vec<[f32; 16]> = match skin.reader(get).read_inverse_bind_matrices() {
            Some(m) => m
                .map(|c| std::array::from_fn(|i| c[i / 4][i % 4]))
                .collect(),
            None => vec![IDENTITY; joints.len()],
        };
        let data = SkinData {
            joint_names: joints
                .iter()
                .map(|j| names.nodes[j.index()].clone())
                .collect(),
            joints: joints
                .iter()
                .map(|j| entity(entities, j.index()))
                .collect::<Result<_, _>>()?,
            inverse_bind,
        };
        let bytes = encode(&data).map_err(|e| e.to_string())?;
        out.artifacts.insert(
            format!("skin/{}", skin.index()),
            Artifact {
                kind: AssetKind::Skin,
                bytes,
            },
        );
    }
    for anim in doc.animations() {
        let channels = anim
            .channels()
            .map(|c| channel(&c, get, entities))
            .collect::<Result<Vec<_>, String>>()?;
        let duration = channels
            .iter()
            .flat_map(|c| c.times.last().copied())
            .fold(0.0f32, f32::max);
        let data = ClipData {
            name: names.clips[anim.index()].clone(),
            duration,
            channels,
        };
        let bytes = encode(&data).map_err(|e| e.to_string())?;
        out.artifacts.insert(
            format!("clip/{}", names.clips[anim.index()]),
            Artifact {
                kind: AssetKind::Clip,
                bytes,
            },
        );
    }
    Ok(())
}

fn channel<'a, 's, F>(
    c: &gltf::animation::Channel<'a>,
    get: F,
    entities: &[EntityId],
) -> Result<Channel, String>
where
    F: Clone + Fn(gltf::Buffer<'a>) -> Option<&'s [u8]>,
{
    use gltf::animation::util::ReadOutputs;
    let reader = c.reader(get);
    let times: Vec<f32> = reader
        .read_inputs()
        .ok_or("animation channel without inputs")?
        .collect();
    let (path, values): (ChannelPath, Vec<f32>) = match reader
        .read_outputs()
        .ok_or("animation channel without outputs")?
    {
        ReadOutputs::Translations(v) => (ChannelPath::Translation, v.flatten().collect()),
        ReadOutputs::Rotations(v) => (ChannelPath::Rotation, v.into_f32().flatten().collect()),
        ReadOutputs::Scales(v) => (ChannelPath::Scale, v.flatten().collect()),
        ReadOutputs::MorphTargetWeights(v) => (ChannelPath::Weights, v.into_f32().collect()),
    };
    let interpolation = match c.sampler().interpolation() {
        gltf::animation::Interpolation::Linear => Interpolation::Linear,
        gltf::animation::Interpolation::Step => Interpolation::Step,
        gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
    };
    let target = entity(entities, c.target().node().index())?;
    Ok(Channel {
        target,
        path,
        interpolation,
        times,
        values,
    })
}

fn entity(entities: &[EntityId], node: usize) -> Result<EntityId, String> {
    entities
        .get(node)
        .copied()
        .ok_or_else(|| format!("node {node} has no entity"))
}
