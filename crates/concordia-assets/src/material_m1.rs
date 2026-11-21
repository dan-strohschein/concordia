//! M1's flat metallic-roughness `.material.ron` (glTF 2.0 core), read only to migrate it to one
//! slab (ADR-0068 decision 7). It is private: nothing writes this format any more.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::data::AlphaMode;
use crate::material::{Material, Slab};
use crate::reference::AssetRef;

/// The fields and defaults of M1's material file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct M1Material {
    base_color: [f32; 4],
    base_color_texture: Option<AssetRef>,
    metallic: f32,
    roughness: f32,
    metallic_roughness_texture: Option<AssetRef>,
    normal_texture: Option<AssetRef>,
    normal_scale: f32,
    occlusion_texture: Option<AssetRef>,
    occlusion_strength: f32,
    emissive: [f32; 3],
    emissive_strength: f32,
    emissive_texture: Option<AssetRef>,
    alpha_mode: AlphaMode,
    double_sided: bool,
    unlit: bool,
}

impl Default for M1Material {
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

impl M1Material {
    /// The equal one-slab material (its slab is named `surface`).
    pub(crate) fn migrate(self) -> Material {
        let slab = Slab {
            base_color: self.base_color,
            base_color_texture: self.base_color_texture,
            metallic: self.metallic,
            roughness: self.roughness,
            metallic_roughness_texture: self.metallic_roughness_texture,
            normal_texture: self.normal_texture,
            normal_scale: self.normal_scale,
            occlusion_texture: self.occlusion_texture,
            occlusion_strength: self.occlusion_strength,
            emissive: self.emissive,
            emissive_strength: self.emissive_strength,
            emissive_texture: self.emissive_texture,
            ..Slab::default()
        };
        Material {
            alpha_mode: self.alpha_mode,
            double_sided: self.double_sided,
            unlit: self.unlit,
            ..Material::from_slab(slab)
        }
    }
}
