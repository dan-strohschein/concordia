//! Per-frame GPU data: the frame block, instances and materials (all `float4`/`uint4` rows, so
//! Rust and every shader backend agree on the layout).

use std::collections::BTreeMap;

use concordia_assets::{AlphaMode, AssetRef, MaterialData};
use concordia_core::math::{Mat3, Mat4, Vec4};

/// No texture bound (shaders use the factor alone).
pub(crate) const NO_TEXTURE: u32 = u32::MAX;
const FLAG_UNLIT: u32 = 1;
const FLAG_MASK: u32 = 2;

/// Push constants for one forward draw (mirrors `DrawPush` in `forward.slang`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DrawPush {
    /// Frame block buffer.
    pub frame: u32,
    /// Instance buffer.
    pub instances: u32,
    /// Instance index.
    pub instance: u32,
    /// Material buffer.
    pub materials: u32,
    /// Material index.
    pub material: u32,
    /// Index stream.
    pub indices: u32,
    /// Position stream.
    pub positions: u32,
    /// Normal stream.
    pub normals: u32,
    /// Tangent stream.
    pub tangents: u32,
    /// UV stream.
    pub uvs: u32,
}

concordia_shaders::shader_struct!(DrawPush {
    frame,
    instances,
    instance,
    materials,
    material,
    indices,
    positions,
    normals,
    tangents,
    uvs
});

impl DrawPush {
    /// Little-endian bytes for `Command::PushConstants`.
    pub fn bytes(&self) -> Vec<u8> {
        [
            self.frame,
            self.instances,
            self.instance,
            self.materials,
            self.material,
            self.indices,
            self.positions,
            self.normals,
            self.tangents,
            self.uvs,
        ]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect()
    }
}

/// Appends `vectors` as little-endian floats.
pub(crate) fn push_rows(out: &mut Vec<u8>, rows: &[Vec4]) {
    out.extend(
        rows.iter()
            .flat_map(|r| r.to_array())
            .flat_map(f32::to_le_bytes),
    );
}

fn columns(m: &Mat4) -> [Vec4; 4] {
    [m.x_axis, m.y_axis, m.z_axis, m.w_axis]
}

/// `Frame`: view-projection, eye, light direction, light radiance (exposed), params.
pub(crate) fn frame_block(view_proj: &Mat4, eye: Vec4, light_dir: Vec4, radiance: Vec4) -> Vec<u8> {
    let mut out = Vec::with_capacity(128);
    push_rows(&mut out, &columns(view_proj));
    push_rows(&mut out, &[eye, light_dir, radiance, Vec4::ZERO]);
    out
}

/// `Instance`: model matrix and normal matrix (inverse transpose).
pub(crate) fn instance_block(out: &mut Vec<u8>, model: &Mat4) {
    let normal = Mat4::from_mat3(Mat3::from_mat4(*model).inverse().transpose());
    push_rows(out, &columns(model));
    push_rows(out, &columns(&normal));
}

/// `Material` rows for `material`, given its textures' bindless indices.
pub(crate) fn material_block(
    out: &mut Vec<u8>,
    material: &MaterialData,
    textures: &BTreeMap<AssetRef, u32>,
) {
    let tex = |r: &Option<AssetRef>| {
        r.as_ref()
            .and_then(|r| textures.get(r))
            .copied()
            .unwrap_or(NO_TEXTURE)
    };
    let (mut flags, mut cutoff) = (0, 0.0);
    if material.unlit {
        flags |= FLAG_UNLIT;
    }
    if let AlphaMode::Mask { cutoff: c } = material.alpha_mode {
        flags |= FLAG_MASK;
        cutoff = c;
    }
    let e = material.emissive.map(|c| c * material.emissive_strength);
    push_rows(
        out,
        &[
            Vec4::from_array(material.base_color),
            Vec4::new(e[0], e[1], e[2], cutoff),
            Vec4::new(
                material.metallic,
                material.roughness,
                material.normal_scale,
                material.occlusion_strength,
            ),
        ],
    );
    let words = [
        tex(&material.base_color_texture),
        tex(&material.metallic_roughness_texture),
        tex(&material.normal_texture),
        tex(&material.occlusion_texture),
        tex(&material.emissive_texture),
        flags,
        0,
        0,
    ];
    out.extend(words.iter().flat_map(|w| w.to_le_bytes()));
}

/// Textures a material references.
pub(crate) fn material_textures(material: &MaterialData) -> impl Iterator<Item = &AssetRef> {
    [
        &material.base_color_texture,
        &material.metallic_roughness_texture,
        &material.normal_texture,
        &material.occlusion_texture,
        &material.emissive_texture,
    ]
    .into_iter()
    .flatten()
}
