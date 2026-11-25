//! Summary statistics of a meshlet build (`concordia cluster stats`, `asset.meshlet_stats`).

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::MeshletMesh;

/// Minimum, mean and maximum of a quantity over meshlets (all 0 without meshlets).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct Spread {
    /// Smallest value.
    pub min: f32,
    /// Mean value.
    pub mean: f32,
    /// Largest value.
    pub max: f32,
}

/// How a mesh was split into meshlets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct MeshletStats {
    /// Triangles in the source mesh.
    pub source_triangles: u32,
    /// Vertices in the source mesh.
    pub source_vertices: u32,
    /// Meshlets built.
    pub meshlets: u32,
    /// Triangle limit per meshlet (64 or 128).
    pub max_triangles: u32,
    /// Vertex limit per meshlet.
    pub max_vertices: u32,
    /// The build's cone weight.
    pub cone_weight: f32,
    /// Triangles per meshlet.
    pub triangles_per_meshlet: Spread,
    /// Mean triangles per meshlet over the limit (1 is full).
    pub triangle_fill: f32,
    /// Vertices per meshlet.
    pub vertices_per_meshlet: Spread,
    /// Bounding-sphere radius (mesh units).
    pub radius: Spread,
    /// Share of meshlets whose normal cone can cull from some view.
    pub cullable_share: f32,
    /// Mean half-angle of the cullable meshlets' normal cones, in degrees (none if none can cull).
    pub mean_cone_half_angle_deg: Option<f32>,
    /// Size of the encoded meshlets.
    pub encoded_bytes: u64,
}

impl MeshletStats {
    /// The statistics of `mesh`, whose encoded form is `encoded_bytes` long.
    pub fn of(mesh: &MeshletMesh, encoded_bytes: usize) -> Self {
        let m = &mesh.meshlets;
        let triangles = Spread::of(m.iter().map(|x| f32::from(x.triangle_count)));
        let max_triangles = mesh.settings.size().triangles();
        let cullable: Vec<f32> = m
            .iter()
            .filter(|x| x.cone.can_cull())
            .map(|x| {
                (f32::from(x.cone.cutoff) / 127.0)
                    .clamp(-1.0, 1.0)
                    .asin()
                    .to_degrees()
            })
            .collect();
        Self {
            source_triangles: mesh.source_triangles,
            source_vertices: mesh.source_vertices,
            meshlets: m.len() as u32,
            max_triangles,
            max_vertices: mesh.settings.max_vertices(),
            cone_weight: mesh.settings.cone_weight(),
            triangles_per_meshlet: triangles,
            triangle_fill: triangles.mean / max_triangles as f32,
            vertices_per_meshlet: Spread::of(m.iter().map(|x| f32::from(x.vertex_count))),
            radius: Spread::of(m.iter().map(|x| x.bounds.radius)),
            cullable_share: share(cullable.len(), m.len()),
            mean_cone_half_angle_deg: (!cullable.is_empty())
                .then(|| cullable.iter().sum::<f32>() / cullable.len() as f32),
            encoded_bytes: encoded_bytes as u64,
        }
    }
}

impl Spread {
    /// The spread of `values` (all 0 when empty).
    pub fn of(values: impl Iterator<Item = f32>) -> Self {
        let (mut min, mut max, mut sum, mut n) = (f32::MAX, f32::MIN, 0.0f64, 0usize);
        for v in values {
            min = min.min(v);
            max = max.max(v);
            sum += f64::from(v);
            n += 1;
        }
        match n {
            0 => Self::default(),
            _ => Self {
                min,
                mean: (sum / n as f64) as f32,
                max,
            },
        }
    }
}

/// `part / whole`, 0 when `whole` is 0.
fn share(part: usize, whole: usize) -> f32 {
    if whole == 0 {
        0.0
    } else {
        part as f32 / whole as f32
    }
}
