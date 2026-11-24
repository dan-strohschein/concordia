//! Primitive meshes built in code (shared by the built-in assets and `concordia-gen`).

use std::f32::consts::PI;

use crate::data::MeshData;

/// Position, normal, tangent, uv.
pub type Vertex = ([f32; 3], [f32; 3], [f32; 4], [f32; 2]);

/// Builds a mesh (with bounds) from vertices and a triangle list.
pub fn mesh(vertices: Vec<Vertex>, indices: Vec<u32>) -> MeshData {
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

/// A box of `size` centred on the origin: 24 vertices (flat faces), 12 triangles.
pub fn cube(size: [f32; 3]) -> MeshData {
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
            let p = std::array::from_fn(|i| 0.5 * (n[i] + su * u[i] + sv * v[i]) * size[i]);
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

/// A `size` plane on XZ facing +Y, centred on the origin.
pub fn plane(size: [f32; 2]) -> MeshData {
    let up = [0.0, 1.0, 0.0];
    let vertices = [(-0.5, 0.5), (0.5, 0.5), (0.5, -0.5), (-0.5, -0.5)]
        .into_iter()
        .map(|(x, z)| {
            let p = [x * size[0], 0.0, z * size[1]];
            (p, up, [1.0, 0.0, 0.0, 1.0], [x + 0.5, z + 0.5])
        })
        .collect();
    mesh(vertices, vec![0, 1, 2, 0, 2, 3])
}

/// A UV sphere of `radius` with `segments` around and `rings` from pole to pole.
pub fn sphere(radius: f32, segments: u32, rings: u32) -> MeshData {
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
            let p = n.map(|c| c * radius);
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
