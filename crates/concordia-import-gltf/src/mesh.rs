//! Primitives → `MeshData` (`mesh/<mesh>/<primitive>`), generating normals and tangents when missing.

use concordia_assets::{Artifact, AssetKind, ImportOutput, MeshData, encode};

use crate::names::Names;

/// Imports every triangle primitive.
pub(crate) fn meshes(
    doc: &gltf::Document,
    buffers: &[Vec<u8>],
    names: &Names,
    out: &mut ImportOutput,
) -> Result<(), String> {
    for mesh in doc.meshes() {
        for (p, primitive) in mesh.primitives().enumerate() {
            let sub = format!("mesh/{}/{p}", names.meshes[mesh.index()]);
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                return Err(format!(
                    "{sub}: only triangle primitives are supported, found {:?}",
                    primitive.mode()
                ));
            }
            let data = primitive_data(&primitive, buffers).map_err(|e| format!("{sub}: {e}"))?;
            out.artifacts.insert(
                sub,
                Artifact {
                    kind: AssetKind::Mesh,
                    bytes: encode(&data).map_err(|e| e.to_string())?,
                },
            );
        }
    }
    Ok(())
}

fn primitive_data(
    primitive: &gltf::Primitive<'_>,
    buffers: &[Vec<u8>],
) -> Result<MeshData, String> {
    let reader = primitive.reader(|b| buffers.get(b.index()).map(Vec::as_slice));
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or("no POSITION attribute")?
        .collect();
    let n = positions.len();
    let indices: Vec<u32> = reader
        .read_indices()
        .map_or_else(|| (0..n as u32).collect(), |i| i.into_u32().collect());
    let mut mesh = MeshData {
        aabb: MeshData::compute_aabb(&positions),
        normals: reader
            .read_normals()
            .map(Iterator::collect)
            .unwrap_or_default(),
        tangents: reader
            .read_tangents()
            .map(Iterator::collect)
            .unwrap_or_default(),
        uv0: reader
            .read_tex_coords(0)
            .map(|t| t.into_f32().collect())
            .unwrap_or_default(),
        uv1: reader
            .read_tex_coords(1)
            .map(|t| t.into_f32().collect())
            .unwrap_or_default(),
        colors: reader
            .read_colors(0)
            .map(|c| c.into_rgba_f32().collect())
            .unwrap_or_default(),
        joints: reader
            .read_joints(0)
            .map(|j| j.into_u16().collect())
            .unwrap_or_default(),
        weights: reader
            .read_weights(0)
            .map(|w| w.into_f32().collect())
            .unwrap_or_default(),
        positions,
        indices,
        meshlets: Vec::new(),
    };
    if mesh.normals.len() != n {
        mesh.normals = smooth_normals(&mesh.positions, &mesh.indices);
    }
    if mesh.uv0.len() != n {
        mesh.uv0 = vec![[0.0; 2]; n];
    }
    if mesh.tangents.len() != n {
        generate_tangents(&mut mesh)?;
    }
    mesh.validate().map_err(|e| e.to_string())?;
    Ok(mesh)
}

/// Area-weighted vertex normals.
fn smooth_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut sums = vec![[0.0f32; 3]; positions.len()];
    for tri in indices.as_chunks::<3>().0 {
        let [a, b, c] = tri.map(|i| positions[i as usize]);
        let (u, v) = (sub(b, a), sub(c, a));
        let face = cross(u, v);
        for i in tri {
            let s = &mut sums[*i as usize];
            (0..3).for_each(|k| s[k] += face[k]);
        }
    }
    sums.into_iter()
        .map(|s| normalize(s).unwrap_or([0.0, 1.0, 0.0]))
        .collect()
}

/// MikkTSpace tangents (or any perpendicular when the mesh has no UVs).
pub fn generate_tangents(mesh: &mut MeshData) -> Result<(), String> {
    let n = mesh.positions.len();
    mesh.tangents = mesh.normals.iter().map(|nrm| perpendicular(*nrm)).collect();
    if mesh.uv0.iter().all(|uv| *uv == [0.0, 0.0]) || mesh.indices.is_empty() {
        return Ok(());
    }
    let mut geometry = Geometry { mesh, n };
    bevy_mikktspace::generate_tangents(&mut geometry).map_err(|e| format!("MikkTSpace: {e:?}"))
}

struct Geometry<'a> {
    mesh: &'a mut MeshData,
    n: usize,
}

impl Geometry<'_> {
    fn vertex(&self, face: usize, vert: usize) -> usize {
        (self.mesh.indices[face * 3 + vert] as usize).min(self.n.saturating_sub(1))
    }
}

impl bevy_mikktspace::Geometry for Geometry<'_> {
    fn num_faces(&self) -> usize {
        self.mesh.indices.len() / 3
    }

    fn num_vertices_of_face(&self, _: usize) -> usize {
        3
    }

    fn position(&self, face: usize, vert: usize) -> [f32; 3] {
        self.mesh.positions[self.vertex(face, vert)]
    }

    fn normal(&self, face: usize, vert: usize) -> [f32; 3] {
        self.mesh.normals[self.vertex(face, vert)]
    }

    /// glTF's V axis points down; MikkTSpace assumes up, so V is flipped to get glTF's handedness (w).
    fn tex_coord(&self, face: usize, vert: usize) -> [f32; 2] {
        let [u, v] = self.mesh.uv0[self.vertex(face, vert)];
        [u, 1.0 - v]
    }

    fn set_tangent(
        &mut self,
        tangent_space: Option<bevy_mikktspace::TangentSpace>,
        face: usize,
        vert: usize,
    ) {
        if let Some(t) = tangent_space {
            let v = self.vertex(face, vert);
            self.mesh.tangents[v] = t.tangent_encoded();
        }
    }
}

fn perpendicular(n: [f32; 3]) -> [f32; 4] {
    let axis = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let t = normalize(cross(axis, n)).unwrap_or([1.0, 0.0, 0.0]);
    [t[0], t[1], t[2], 1.0]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> Option<[f32; 3]> {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (len > 1e-12).then(|| v.map(|c| c / len))
}
