//! Derived data stored in the DDC (postcard-encoded) and consumed by the renderer.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::AssetError;
use crate::reference::AssetRef;

/// Pixel format of imported textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureFormat {
    /// 8-bit RGBA, sRGB-encoded (color).
    Rgba8Srgb,
    /// 8-bit RGBA, linear (normals, masks).
    Rgba8Linear,
    /// 16-bit float RGBA (HDR).
    Rgba16Float,
}

impl TextureFormat {
    /// Bytes per pixel.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba8Srgb | Self::Rgba8Linear => 4,
            Self::Rgba16Float => 8,
        }
    }
}

/// A 2D texture with its mip chain (mip 0 first, tightly packed rows).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureData {
    /// Width of mip 0.
    pub width: u32,
    /// Height of mip 0.
    pub height: u32,
    /// Pixel format.
    pub format: TextureFormat,
    /// Mip levels.
    pub mips: Vec<Vec<u8>>,
}

/// A triangle mesh as de-interleaved streams (pulled from bindless buffers by shaders).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MeshData {
    /// Positions.
    pub positions: Vec<[f32; 3]>,
    /// Unit normals.
    pub normals: Vec<[f32; 3]>,
    /// Tangents (xyz) with bitangent sign (w).
    pub tangents: Vec<[f32; 4]>,
    /// First UV set.
    pub uv0: Vec<[f32; 2]>,
    /// Second UV set (optional).
    pub uv1: Vec<[f32; 2]>,
    /// Vertex colors (optional).
    pub colors: Vec<[f32; 4]>,
    /// Skin joint indices (optional).
    pub joints: Vec<[u16; 4]>,
    /// Skin weights (optional).
    pub weights: Vec<[f32; 4]>,
    /// Triangle list indices.
    pub indices: Vec<u32>,
    /// Axis-aligned bounds (min, max).
    pub aabb: ([f32; 3], [f32; 3]),
    /// Meshlets for virtualized geometry (empty until M4).
    pub meshlets: Vec<u8>,
}

impl MeshData {
    /// Checks stream lengths and index range.
    pub fn validate(&self) -> Result<(), AssetError> {
        let n = self.positions.len();
        let bad = |what: &str| Err(AssetError::Decode(format!("mesh: {what}")));
        if n == 0 || self.indices.is_empty() || !self.indices.len().is_multiple_of(3) {
            return bad("needs vertices and a whole number of triangles");
        }
        for (name, len) in [
            ("normals", self.normals.len()),
            ("tangents", self.tangents.len()),
            ("uv0", self.uv0.len()),
        ] {
            if len != n {
                return bad(&format!("{name} has {len} entries for {n} positions"));
            }
        }
        for (name, len) in [
            ("uv1", self.uv1.len()),
            ("colors", self.colors.len()),
            ("joints", self.joints.len()),
            ("weights", self.weights.len()),
        ] {
            if len != 0 && len != n {
                return bad(&format!("{name} has {len} entries for {n} positions"));
            }
        }
        match self.indices.iter().all(|i| (*i as usize) < n) {
            true => Ok(()),
            false => bad("index out of range"),
        }
    }

    /// Bounds of the positions.
    pub fn compute_aabb(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
        positions
            .iter()
            .fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
                (
                    std::array::from_fn(|i| lo[i].min(p[i])),
                    std::array::from_fn(|i| hi[i].max(p[i])),
                )
            })
    }
}

/// How alpha is used.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub enum AlphaMode {
    /// Fully opaque.
    #[default]
    Opaque,
    /// Alpha-tested at a cutoff.
    Mask {
        /// Cutoff (glTF default 0.5).
        cutoff: f32,
    },
    /// Alpha-blended (sorted back to front).
    Blend,
}

/// A metallic-roughness material (glTF 2.0 core); the contents of a `.material.ron` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct MaterialData {
    /// Linear base color and alpha.
    #[reflect(widget = "color")]
    pub base_color: [f32; 4],
    /// Base color texture (sRGB).
    pub base_color_texture: Option<AssetRef>,
    /// Metalness 0..1.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub metallic: f32,
    /// Perceptual roughness 0..1.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub roughness: f32,
    /// Metallic (B) and roughness (G) texture (linear).
    pub metallic_roughness_texture: Option<AssetRef>,
    /// Tangent-space normal map (linear).
    pub normal_texture: Option<AssetRef>,
    /// Normal map strength.
    pub normal_scale: f32,
    /// Ambient occlusion texture (R, linear).
    pub occlusion_texture: Option<AssetRef>,
    /// Occlusion strength.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub occlusion_strength: f32,
    /// Linear emissive color.
    #[reflect(widget = "color")]
    pub emissive: [f32; 3],
    /// Emissive multiplier (KHR_materials_emissive_strength).
    #[reflect(range(min = 0.0, max = 1000000.0))]
    pub emissive_strength: f32,
    /// Emissive texture (sRGB).
    pub emissive_texture: Option<AssetRef>,
    /// Alpha handling.
    pub alpha_mode: AlphaMode,
    /// Render both faces.
    pub double_sided: bool,
    /// Ignore lighting (KHR_materials_unlit).
    pub unlit: bool,
}

impl Default for MaterialData {
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            base_color_texture: None,
            metallic: 1.0,
            roughness: 1.0,
            metallic_roughness_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            emissive: [0.0; 3],
            emissive_strength: 1.0,
            emissive_texture: None,
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            unlit: false,
        }
    }
}

/// Draws an imported or built-in mesh with a material (read by the renderer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct MeshRenderer {
    /// The mesh.
    pub mesh: AssetRef,
    /// The material.
    pub material: AssetRef,
    /// Casts shadows.
    #[serde(default = "yes")]
    pub cast_shadows: bool,
}

fn yes() -> bool {
    true
}

/// A skin: joints (prefab entity ids) and inverse bind matrices (column-major).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkinData {
    /// Joint names in joint-index order.
    pub joint_names: Vec<String>,
    /// Joint entity ids in the prefab, in joint-index order.
    pub joints: Vec<concordia_core::id::EntityId>,
    /// Inverse bind matrices, one per joint.
    pub inverse_bind: Vec<[f32; 16]>,
}

/// What an animation channel drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelPath {
    /// Translation (vec3).
    Translation,
    /// Rotation (quat xyzw).
    Rotation,
    /// Scale (vec3).
    Scale,
    /// Morph target weights.
    Weights,
}

/// How keyframes are interpolated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpolation {
    /// Linear (slerp for rotations).
    Linear,
    /// Hold the previous key.
    Step,
    /// Cubic spline (values hold in-tangent, value, out-tangent).
    CubicSpline,
}

/// One animated property of one node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    /// Target entity id in the prefab.
    pub target: concordia_core::id::EntityId,
    /// Property.
    pub path: ChannelPath,
    /// Interpolation.
    pub interpolation: Interpolation,
    /// Key times in seconds.
    pub times: Vec<f32>,
    /// Flattened key values.
    pub values: Vec<f32>,
}

/// An animation clip (played from M2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipData {
    /// Clip name.
    pub name: String,
    /// Length in seconds.
    pub duration: f32,
    /// Channels.
    pub channels: Vec<Channel>,
}

/// A punctual light from an imported file (turned into light components when rendering arrives).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LightData {
    /// Directional light (illuminance in lux).
    Directional {
        /// Linear color.
        color: [f32; 3],
        /// Lux.
        intensity: f32,
    },
    /// Point light (candela).
    Point {
        /// Linear color.
        color: [f32; 3],
        /// Candela.
        intensity: f32,
        /// Range (None = infinite).
        range: Option<f32>,
    },
    /// Spot light (candela).
    Spot {
        /// Linear color.
        color: [f32; 3],
        /// Candela.
        intensity: f32,
        /// Range (None = infinite).
        range: Option<f32>,
        /// Inner cone angle (radians).
        inner: f32,
        /// Outer cone angle (radians).
        outer: f32,
    },
}

/// A camera from an imported file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CameraData {
    /// Perspective (vertical fov in radians).
    Perspective {
        /// Vertical field of view.
        yfov: f32,
        /// Near plane.
        znear: f32,
        /// Far plane (None = infinite).
        zfar: Option<f32>,
    },
    /// Orthographic.
    Orthographic {
        /// Half width.
        xmag: f32,
        /// Half height.
        ymag: f32,
        /// Near plane.
        znear: f32,
        /// Far plane.
        zfar: f32,
    },
}
