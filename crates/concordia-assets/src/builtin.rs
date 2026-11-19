//! Built-in assets with fixed names (`builtin:mesh/cube`, ...), generated in code.

use std::f32::consts::PI;

use crate::data::{MaterialData, MeshData, TextureData, TextureFormat};
use crate::reference::AssetRef;

/// A built-in asset's data.
#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinAsset {
    /// A mesh.
    Mesh(MeshData),
    /// A texture.
    Texture(TextureData),
    /// A material.
    Material(MaterialData),
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
            "material" if name == "default" => Some(BuiltinAsset::Material(MaterialData {
                metallic: 0.0,
                roughness: 0.5,
                ..MaterialData::default()
            })),
            _ => None,
        }
    }

    /// `cube` (1 m), `plane` (1 m, +Y up) or `sphere` (radius 0.5).
    pub fn mesh(name: &str) -> Option<MeshData> {
        match name {
            "cube" => Some(cube()),
            "plane" => Some(plane()),
            "sphere" => Some(sphere(32, 16)),
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

/// Position, normal, tangent, uv.
type Vertex = ([f32; 3], [f32; 3], [f32; 4], [f32; 2]);

/// Builds a mesh from vertices and indices.
fn mesh(vertices: Vec<Vertex>, indices: Vec<u32>) -> MeshData {
    let positions: Vec<[f32; 3]> = vertices.iter().map(|v| v.0).collect();
    MeshData {
        aabb: MeshData::compute_aabb(&positions),
        positions,
        normals: vertices.iter().map(|v| v.1).collect(),
        tangents: vertices.iter().map(|v| v.2).collect(),
        uv0: vertices.iter().map(|v| v.3).collect(),
        indices,
        ..MeshData::default()
    }
}

fn cube() -> MeshData {
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (n, u, v) in faces {
        let base = vertices.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = std::array::from_fn(|i| 0.5 * (n[i] + su * u[i] + sv * v[i]));
            vertices.push((
                p,
                n,
                [u[0], u[1], u[2], 1.0],
                [(su + 1.0) / 2.0, (1.0 - sv) / 2.0],
            ));
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    mesh(vertices, indices)
}

fn plane() -> MeshData {
    let up = [0.0, 1.0, 0.0];
    let vertices = [(-0.5, 0.5), (0.5, 0.5), (0.5, -0.5), (-0.5, -0.5)]
        .into_iter()
        .map(|(x, z)| ([x, 0.0, z], up, [1.0, 0.0, 0.0, 1.0], [x + 0.5, z + 0.5]))
        .collect();
    mesh(vertices, vec![0, 1, 2, 0, 2, 3])
}

fn sphere(segments: u32, rings: u32) -> MeshData {
    let mut vertices = Vec::new();
    for r in 0..=rings {
        let theta = PI * r as f32 / rings as f32;
        for s in 0..=segments {
            let phi = 2.0 * PI * s as f32 / segments as f32;
            let n = [
                theta.sin() * phi.cos(),
                theta.cos(),
                theta.sin() * phi.sin(),
            ];
            let t = [-phi.sin(), 0.0, phi.cos(), 1.0];
            let p = n.map(|c| c * 0.5);
            vertices.push((
                p,
                n,
                t,
                [s as f32 / segments as f32, r as f32 / rings as f32],
            ));
        }
    }
    let row = segments + 1;
    let indices = (0..rings)
        .flat_map(|r| {
            (0..segments).flat_map(move |s| {
                let (a, b) = (r * row + s, (r + 1) * row + s);
                [a, a + 1, b, a + 1, b + 1, b]
            })
        })
        .collect();
    mesh(vertices, indices)
}
