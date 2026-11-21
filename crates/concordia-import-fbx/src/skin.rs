//! Skin deformers → `SkinData` (`skin/<index>`): bone entities, names and inverse bind matrices.

use std::collections::{BTreeMap, BTreeSet};

use concordia_assets::{AssetKind, ImportOutput, SkinData};
use concordia_core::id::EntityId;
use concordia_import_model::insert_artifact;

use crate::ffi::{Mesh, Scene};
use crate::names::Names;

/// Imports every skin a mesh uses; returns each skin's cluster (joint) count.
pub(crate) fn skins(
    scene: &Scene<'_>,
    meshes: &[Mesh],
    names: &Names,
    entities: &[Option<EntityId>],
    out: &mut ImportOutput,
) -> Result<BTreeMap<usize, usize>, String> {
    let used: BTreeSet<usize> = meshes.iter().filter_map(|m| m.skin).collect();
    let mut counts = BTreeMap::new();
    for skin in used {
        let sub = format!("skin/{skin}");
        let clusters = scene
            .clusters(skin)
            .ok_or_else(|| format!("{sub}: a cluster has no bone"))?;
        if clusters.len() > usize::from(u16::MAX) + 1 {
            return Err(format!("{sub}: {} joints (at most 65536)", clusters.len()));
        }
        let mut data = SkinData {
            joint_names: Vec::with_capacity(clusters.len()),
            joints: Vec::with_capacity(clusters.len()),
            inverse_bind: Vec::with_capacity(clusters.len()),
        };
        for c in &clusters {
            let joint = entities
                .get(c.bone)
                .copied()
                .flatten()
                .ok_or_else(|| format!("{sub}: bone {} is not a prefab node", c.bone))?;
            data.joint_names.push(names.nodes[c.bone].clone());
            data.joints.push(joint);
            data.inverse_bind.push(c.geometry_to_bone);
        }
        insert_artifact(out, sub, AssetKind::Skin, &data).map_err(|e| e.to_string())?;
        counts.insert(skin, clusters.len());
    }
    Ok(counts)
}
