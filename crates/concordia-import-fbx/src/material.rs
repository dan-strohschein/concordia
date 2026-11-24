//! Texture files → textures (`texture/<file stem>`) and materials (`material/<name>`, one `Slab` each).

use std::collections::BTreeMap;

use concordia_assets::{
    AlphaMode, AssetError, AssetKind, AssetRef, ImportOutput, Material, Slab, SubIds, decode_image,
};
use concordia_import_model::{TextureUsage, insert_artifact};

use crate::ffi::{MaterialValues, TextureFile};
use crate::names::Names;

/// Decodes every texture file a material uses (embedded bytes, else the file next to the source).
/// A missing external file is skipped with a warning; returns the file index → sub-path map.
pub(crate) fn textures(
    files: &[TextureFile],
    materials: &[MaterialValues],
    names: &Names,
    read_relative: &dyn Fn(&str) -> Result<Vec<u8>, AssetError>,
    out: &mut ImportOutput,
) -> Result<BTreeMap<usize, String>, String> {
    let mut paths = BTreeMap::new();
    for (index, usage) in usages(materials) {
        let file = files
            .get(index)
            .ok_or_else(|| format!("missing texture file {index}"))?;
        let Some(bytes) = file_bytes(file, read_relative) else {
            continue;
        };
        let sub = format!("texture/{}", names.textures[index]);
        let texture = decode_image(&bytes, &usage.settings()).map_err(|e| format!("{sub}: {e}"))?;
        insert_artifact(out, sub.clone(), AssetKind::Texture, &texture)
            .map_err(|e| e.to_string())?;
        paths.insert(index, sub);
    }
    Ok(paths)
}

fn file_bytes(
    file: &TextureFile,
    read_relative: &dyn Fn(&str) -> Result<Vec<u8>, AssetError>,
) -> Option<Vec<u8>> {
    if !file.content.is_empty() {
        return Some(file.content.clone());
    }
    let path = file.relative_filename.replace('\\', "/");
    match read_relative(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            tracing::warn!(target: "concordia::assets", texture = %path, error = %e, "FBX texture not found; the material keeps its factor");
            None
        }
    }
}

/// The first use of each file (base color and emissive are color; normal maps are normals).
fn usages(materials: &[MaterialValues]) -> BTreeMap<usize, TextureUsage> {
    let mut map = BTreeMap::new();
    for m in materials {
        for (file, usage) in [
            (m.base_color_texture, TextureUsage::Color),
            (m.emissive_texture, TextureUsage::Color),
            (m.normal_texture, TextureUsage::Normal),
        ] {
            if let Some(file) = file {
                map.entry(file).or_insert(usage);
            }
        }
    }
    map
}

/// Imports every material as one slab.
pub(crate) fn materials(
    materials: &[MaterialValues],
    images: &BTreeMap<usize, String>,
    names: &Names,
    emission_scale: f32,
    sub_ids: &SubIds,
    out: &mut ImportOutput,
) -> Result<(), AssetError> {
    let parent = sub_ids.parent();
    let texture = |file: Option<usize>| -> Option<AssetRef> {
        format!("asset:{parent}#{}", images.get(&file?)?)
            .parse()
            .ok()
    };
    for (index, m) in materials.iter().enumerate() {
        let mut slab = slab(m, &texture);
        slab.emissive_strength *= emission_scale;
        let material = Material {
            alpha_mode: if slab.base_color[3] < 1.0 {
                AlphaMode::Blend
            } else {
                AlphaMode::Opaque
            },
            ..Material::from_slab(slab)
        };
        let sub = format!("material/{}", names.materials[index]);
        material
            .validate()
            .map_err(|e| AssetError::Decode(format!("{sub}: {e}")))?;
        insert_artifact(out, sub, AssetKind::Material, &material)?;
    }
    Ok(())
}

/// ufbx's PBR values (sanitized into the slab's ranges) and textures as one slab.
fn slab(m: &MaterialValues, texture: &dyn Fn(Option<usize>) -> Option<AssetRef>) -> Slab {
    let defaults = Slab::default();
    let unit = |v: f32, default: f32| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            default
        }
    };
    let glows = m.emissive_strength.is_finite()
        && m.emissive_strength > 0.0
        && m.emissive.iter().any(|c| *c > 0.0);
    let (emissive, emissive_strength) = match glows {
        true => (
            m.emissive.map(|c| unit(c, 0.0)),
            m.emissive_strength.min(1.0e6),
        ),
        false => ([0.0; 3], 1.0),
    };
    Slab {
        base_color: std::array::from_fn(|i| unit(m.base_color[i], 1.0)),
        base_color_texture: texture(m.base_color_texture),
        metallic: unit(m.metallic, defaults.metallic),
        roughness: unit(m.roughness, defaults.roughness),
        normal_texture: texture(m.normal_texture),
        emissive,
        emissive_strength,
        emissive_texture: texture(m.emissive_texture),
        ..defaults
    }
}
