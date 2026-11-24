//! Built-in assets with fixed names (`builtin:mesh/cube`, ...), generated in code.

use crate::data::{MeshData, TextureData, TextureFormat};
use crate::material::{Material, Slab};
use crate::reference::AssetRef;
use crate::shapes;

/// A built-in asset's data.
#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinAsset {
    /// A mesh.
    Mesh(MeshData),
    /// A texture.
    Texture(TextureData),
    /// A material.
    Material(Material),
}

/// Lookup of built-in assets.
#[derive(Debug, Clone, Copy)]
pub struct Builtin;

impl Builtin {
    /// The built-in a reference names, if any.
    pub fn of(reference: &AssetRef) -> Option<BuiltinAsset> {
        let AssetRef::Builtin { kind, name } = reference else {
            return None;
        };
        match kind.as_str() {
            "mesh" => Self::mesh(name).map(BuiltinAsset::Mesh),
            "texture" => Self::texture(name).map(BuiltinAsset::Texture),
            "material" if name == "default" => {
                Some(BuiltinAsset::Material(Material::from_slab(Slab {
                    base_color: [1.0; 4],
                    ..Slab::default()
                })))
            }
            _ => None,
        }
    }

    /// `cube` (1 m), `plane` (1 m, +Y up) or `sphere` (radius 0.5).
    pub fn mesh(name: &str) -> Option<MeshData> {
        match name {
            "cube" => Some(shapes::cube([1.0; 3])),
            "plane" => Some(shapes::plane([1.0, 1.0])),
            "sphere" => Some(shapes::sphere(0.5, 32, 16)),
            _ => None,
        }
    }

    /// `white`, `black` or `normal_flat` (1×1 linear RGBA8).
    pub fn texture(name: &str) -> Option<TextureData> {
        let pixel = match name {
            "white" => [255, 255, 255, 255],
            "black" => [0, 0, 0, 255],
            "normal_flat" => [128, 128, 255, 255],
            _ => return None,
        };
        Some(TextureData {
            width: 1,
            height: 1,
            format: TextureFormat::Rgba8Linear,
            mips: vec![pixel.to_vec()],
        })
    }
}
