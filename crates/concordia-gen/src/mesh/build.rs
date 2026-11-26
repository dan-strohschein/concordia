//! A mesh under construction: vertices with normals and uvs, triangles oriented outward.

use concordia_assets::MeshData;

/// Vertices and triangles, before they become [`MeshData`].
#[derive(Debug, Default)]
pub(crate) struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

pub(crate) fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt();
    if len > 0.0 {
        v.map(|c| c / len)
    } else {
        [0.0, 1.0, 0.0]
    }
}

impl Builder {
    /// Adds a vertex; returns its index.
    pub fn vertex(&mut self, p: [f32; 3], n: [f32; 3], uv: [f32; 2]) -> u32 {
        self.positions.push(p);
        self.normals.push(normalize(n));
        self.uvs.push(uv);
        (self.positions.len() - 1) as u32
    }

    /// Adds a triangle wound counter-clockwise when seen from outside: its geometric normal is
    /// turned to agree with its vertices' normals (degenerate triangles keep their order).
    pub fn triangle(&mut self, a: u32, b: u32, c: u32) {
        let [pa, pb, pc] = [a, b, c].map(|i| self.positions[i as usize]);
        let face = cross(sub(pb, pa), sub(pc, pa));
        let want = [a, b, c]
            .iter()
            .map(|&i| self.normals[i as usize])
            .fold([0.0; 3], |s, n| [s[0] + n[0], s[1] + n[1], s[2] + n[2]]);
        if dot(face, want) < 0.0 {
            self.indices.extend([a, c, b]);
        } else {
            self.indices.extend([a, b, c]);
        }
    }

    /// Triangles already counter-clockwise from outside, kept as given whatever the vertex
    /// normals say (three indices each).
    pub fn wound_triangles(&mut self, indices: &[u32]) {
        debug_assert!(indices.len().is_multiple_of(3));
        self.indices.extend_from_slice(indices);
    }

    /// Two triangles over the quad `a b c d` (in order around it).
    pub fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.triangle(a, b, c);
        self.triangle(a, c, d);
    }

    /// The finished mesh, with bounds.
    pub fn finish(self) -> MeshData {
        MeshData {
            aabb: MeshData::compute_aabb(&self.positions),
            positions: self.positions,
            normals: self.normals,
            uv0: self.uvs,
            indices: self.indices,
            ..MeshData::default()
        }
    }
}
