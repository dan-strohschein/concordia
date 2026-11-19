//! Per-frame GPU data: the frame block, instances and materials (all `float4`/`uint4` rows, so
//! Rust and every shader backend agree on the layout).

use std::collections::BTreeMap;

use concordia_assets::{AlphaMode, AssetRef, MaterialData};
use concordia_core::math::{Mat3, Mat4, Vec3, Vec4};

use crate::cluster::ClusterGrid;
use crate::shadow::{Cascade, CascadeSettings};

/// No texture bound (shaders use the factor alone).
pub(crate) const NO_TEXTURE: u32 = u32::MAX;
const FLAG_UNLIT: u32 = 1;
const FLAG_MASK: u32 = 2;
const FLAG_BLEND: u32 = 4;

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

/// What the `Frame` block holds.
pub(crate) struct FrameParams<'a> {
    pub(crate) view_proj: Mat4,
    pub(crate) view: Mat4,
    pub(crate) eye: Vec3,
    pub(crate) sun_dir: Vec3,
    /// Sun radiance, already multiplied by exposure.
    pub(crate) sun_radiance: Vec3,
    pub(crate) grid: &'a ClusterGrid,
    pub(crate) size: (u32, u32),
    pub(crate) light_count: u32,
    /// Bindless indices: lights, cluster counts, cluster indices.
    pub(crate) buffers: [u32; 3],
    /// Fitted cascades, their settings and shadow-map indices (None: no shadows).
    pub(crate) shadow: Option<(&'a [Cascade], &'a CascadeSettings, [u32; 4])>,
    /// Image-based lighting (None: no environment).
    pub(crate) ibl: Option<IblParams<'a>>,
    /// Bindless index of the BRDF table.
    pub(crate) lut: u32,
}

/// The environment as the shader sees it.
pub(crate) struct IblParams<'a> {
    pub(crate) sh: &'a [[f32; 3]; 9],
    /// Intensity × exposure.
    pub(crate) scale: f32,
    pub(crate) skybox: bool,
    pub(crate) levels: [u32; 6],
}

/// `Frame` (mirrors `forward.slang`): matrices, eye, sun, cluster depth range, screen, then two
/// `uint4` rows (cluster dims and budget; buffer indices).
pub(crate) fn frame_block(p: &FrameParams<'_>) -> Vec<u8> {
    let (near, far, ortho) = p.grid.depth_range();
    let [dx, dy, dz] = p.grid.settings().dims;
    let mut out = Vec::with_capacity(256);
    push_rows(&mut out, &columns(&p.view_proj));
    push_rows(&mut out, &columns(&p.view));
    push_rows(
        &mut out,
        &[
            p.eye.extend(1.0),
            p.sun_dir.extend(0.0),
            p.sun_radiance.extend(0.0),
            Vec4::new(near, far, if ortho { 1.0 } else { 0.0 }, (far / near).ln()),
            Vec4::new(p.size.0 as f32, p.size.1 as f32, p.light_count as f32, 0.0),
        ],
    );
    let words = [
        dx,
        dy,
        dz,
        p.grid.settings().max_per_cluster,
        p.buffers[0],
        p.buffers[1],
        p.buffers[2],
        0,
    ];
    out.extend(words.iter().flat_map(|w| w.to_le_bytes()));
    shadow_rows(&mut out, p.shadow);
    ibl_rows(&mut out, p);
    out
}

/// Inverse view-projection, SH9, IBL parameters and level / table indices (zeros: off).
fn ibl_rows(out: &mut Vec<u8>, p: &FrameParams<'_>) {
    push_rows(out, &columns(&p.view_proj.inverse()));
    let (mut sh, mut params, mut levels) = ([Vec4::ZERO; 9], Vec4::ZERO, [0u32; 6]);
    if let Some(ibl) = &p.ibl {
        sh = ibl.sh.map(|c| Vec4::new(c[0], c[1], c[2], 0.0));
        params = Vec4::new(
            ibl.scale,
            ibl.levels.len() as f32,
            if ibl.skybox { 1.0 } else { 0.0 },
            1.0,
        );
        levels = ibl.levels;
    }
    push_rows(out, &sh);
    push_rows(out, &[params]);
    let words = [
        levels[0], levels[1], levels[2], levels[3], levels[4], levels[5], p.lut, 0,
    ];
    out.extend(words.iter().flat_map(|w| w.to_le_bytes()));
}

/// Cascade matrices, splits, texel sizes, depth ranges, parameters and map indices (zeros: off).
fn shadow_rows(out: &mut Vec<u8>, shadow: Option<(&[Cascade], &CascadeSettings, [u32; 4])>) {
    let mut matrices = [Vec4::ZERO; 16];
    let (mut splits, mut texel, mut depth) = ([0.0f32; 4], [0.0f32; 4], [1.0f32; 4]);
    let (mut params, mut res, mut maps) = (Vec4::ZERO, Vec4::ZERO, [0u32; 4]);
    if let Some((cascades, settings, indices)) = shadow {
        for (i, c) in cascades.iter().take(4).enumerate() {
            matrices[i * 4..i * 4 + 4].copy_from_slice(&columns(&c.view_proj));
            (splits[i], texel[i], depth[i]) = (c.split_far, c.texel_world, c.depth_range);
        }
        params = Vec4::new(
            settings.depth_bias,
            settings.normal_bias,
            settings.pcf.radius() as f32,
            cascades.len().min(4) as f32,
        );
        res = Vec4::new(settings.resolution as f32, 0.0, 0.0, 0.0);
        maps = indices;
    }
    push_rows(out, &matrices);
    push_rows(
        out,
        &[
            Vec4::from_array(splits),
            Vec4::from_array(texel),
            Vec4::from_array(depth),
            params,
            res,
        ],
    );
    out.extend(maps.iter().flat_map(|w| w.to_le_bytes()));
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
    if material.alpha_mode == AlphaMode::Blend {
        flags |= FLAG_BLEND;
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
