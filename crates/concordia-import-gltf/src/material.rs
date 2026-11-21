//! Images → textures (`texture/<image>`) and materials (`material/<name>`, one `Slab` each).

use std::collections::BTreeMap;

use concordia_assets::{
    AlphaMode, Artifact, AssetError, AssetKind, AssetRef, ImportOutput, Material, Slab, SubIds,
    decode_image, encode,
};
use concordia_import_model::TextureUsage as Usage;

use crate::names::Names;
use crate::source::uri_bytes;

/// Decodes every image used by a material; returns the image index → sub-path map.
pub(crate) fn textures(
    doc: &gltf::Document,
    buffers: &[Vec<u8>],
    read_relative: &dyn Fn(&str) -> Result<Vec<u8>, AssetError>,
    names: &Names,
    out: &mut ImportOutput,
) -> Result<BTreeMap<usize, String>, String> {
    let mut paths = BTreeMap::new();
    for (index, usage) in usages(doc) {
        let image = doc
            .images()
            .nth(index)
            .ok_or_else(|| format!("missing image {index}"))?;
        let bytes = match image.source() {
            gltf::image::Source::View { view, .. } => {
                let buffer = buffers
                    .get(view.buffer().index())
                    .ok_or("image view outside buffers")?;
                buffer
                    .get(view.offset()..view.offset() + view.length())
                    .ok_or("image view out of range")?
                    .to_vec()
            }
            gltf::image::Source::Uri { uri, .. } => uri_bytes(uri, read_relative)?,
        };
        let sub = format!("texture/{}", names.images[index]);
        let texture = decode_image(&bytes, &usage.settings()).map_err(|e| format!("{sub}: {e}"))?;
        out.artifacts.insert(
            sub.clone(),
            Artifact {
                kind: AssetKind::Texture,
                bytes: encode(&texture).map_err(|e| e.to_string())?,
            },
        );
        paths.insert(index, sub);
    }
    Ok(paths)
}

/// The first use of each image (base color and emissive are color; normal maps are normals).
fn usages(doc: &gltf::Document) -> BTreeMap<usize, Usage> {
    let mut map = BTreeMap::new();
    let mut note = |tex: Option<gltf::Texture<'_>>, usage| {
        if let Some(t) = tex {
            map.entry(t.source().index()).or_insert(usage);
        }
    };
    for m in doc.materials() {
        let pbr = m.pbr_metallic_roughness();
        note(pbr.base_color_texture().map(|i| i.texture()), Usage::Color);
        note(m.emissive_texture().map(|i| i.texture()), Usage::Color);
        note(m.normal_texture().map(|i| i.texture()), Usage::Normal);
        note(
            pbr.metallic_roughness_texture().map(|i| i.texture()),
            Usage::Data,
        );
        note(m.occlusion_texture().map(|i| i.texture()), Usage::Data);
    }
    map
}

/// Imports every material.
pub(crate) fn materials(
    doc: &gltf::Document,
    images: &BTreeMap<usize, String>,
    names: &Names,
    sub_ids: &mut SubIds,
    out: &mut ImportOutput,
) -> Result<(), AssetError> {
    let parent = sub_ids.parent();
    let texture = |t: Option<gltf::Texture<'_>>| -> Option<AssetRef> {
        let sub = images.get(&t?.source().index())?;
        format!("asset:{parent}#{sub}").parse().ok()
    };
    for m in doc.materials() {
        let Some(index) = m.index() else { continue };
        let material = Material {
            alpha_mode: alpha_mode(&m),
            double_sided: m.double_sided(),
            unlit: m.unlit(),
            ..Material::from_slab(slab(&m, &texture))
        };
        let sub = format!("material/{}", names.materials[index]);
        out.artifacts.insert(
            sub,
            Artifact {
                kind: AssetKind::Material,
                bytes: encode(&material)?,
            },
        );
    }
    Ok(())
}

/// glTF's metallic-roughness factors and textures as one slab (specular 0.5: F0 0.04).
fn slab(
    m: &gltf::Material<'_>,
    texture: &dyn Fn(Option<gltf::Texture<'_>>) -> Option<AssetRef>,
) -> Slab {
    let pbr = m.pbr_metallic_roughness();
    Slab {
        base_color: pbr.base_color_factor(),
        base_color_texture: texture(pbr.base_color_texture().map(|i| i.texture())),
        metallic: pbr.metallic_factor(),
        roughness: pbr.roughness_factor(),
        metallic_roughness_texture: texture(pbr.metallic_roughness_texture().map(|i| i.texture())),
        normal_texture: texture(m.normal_texture().map(|n| n.texture())),
        normal_scale: m.normal_texture().map_or(1.0, |n| n.scale()),
        occlusion_texture: texture(m.occlusion_texture().map(|o| o.texture())),
        occlusion_strength: m.occlusion_texture().map_or(1.0, |o| o.strength()),
        emissive: m.emissive_factor(),
        emissive_strength: m.emissive_strength().unwrap_or(1.0),
        emissive_texture: texture(m.emissive_texture().map(|i| i.texture())),
        ..Slab::default()
    }
}

fn alpha_mode(m: &gltf::Material<'_>) -> AlphaMode {
    match m.alpha_mode() {
        gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
        gltf::material::AlphaMode::Mask => AlphaMode::Mask {
            cutoff: m.alpha_cutoff().unwrap_or(0.5),
        },
        gltf::material::AlphaMode::Blend => AlphaMode::Blend,
    }
}
