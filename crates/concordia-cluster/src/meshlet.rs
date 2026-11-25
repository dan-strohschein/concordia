//! Meshlets, their bounds and normal cones.

use std::ops::Range;

use crate::MeshletBuildSettings;

/// A bounding sphere.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sphere {
    /// Center.
    pub center: [f32; 3],
    /// Radius.
    pub radius: f32,
}

/// A normal cone in 8-bit SNORM (decode with `x / 127`), rounded so culling stays conservative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NormalCone {
    /// Cone axis.
    pub axis: [i8; 3],
    /// Sine of the normal cone's half-angle (the visibility test's cutoff), rounded up for the
    /// axis's quantization; 127 means "never cull".
    pub cutoff: i8,
}

impl NormalCone {
    /// True if every triangle bounded by `sphere` faces away from `eye` (the sphere form of the
    /// cone test, the one GPU culling uses).
    pub fn rejects(&self, sphere: &Sphere, eye: [f32; 3]) -> bool {
        let axis = self.axis.map(|a| f32::from(a) / 127.0);
        let cutoff = f32::from(self.cutoff) / 127.0;
        let d = [0, 1, 2].map(|i| sphere.center[i] - eye[i]);
        let distance = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        d[0] * axis[0] + d[1] * axis[1] + d[2] * axis[2] >= cutoff * distance + sphere.radius
    }

    /// True if some view can reject this cone (cutoff below 1).
    pub fn can_cull(&self) -> bool {
        self.cutoff < 127
    }
}

/// One meshlet: bounds, cone, material and its ranges in [`MeshletMesh`]'s arrays.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Meshlet {
    /// Bounding sphere of its vertices.
    pub bounds: Sphere,
    /// Normal cone of its triangles.
    pub cone: NormalCone,
    /// The material all its triangles share.
    pub material: u16,
    /// First entry in [`MeshletMesh::vertices`].
    pub vertex_offset: u32,
    /// Entries in [`MeshletMesh::vertices`].
    pub vertex_count: u8,
    /// First byte in [`MeshletMesh::triangles`].
    pub triangle_offset: u32,
    /// Triangles (3 bytes each).
    pub triangle_count: u8,
}

impl Meshlet {
    /// Its range in [`MeshletMesh::vertices`].
    pub fn vertex_range(&self) -> Range<usize> {
        let start = self.vertex_offset as usize;
        start..start + usize::from(self.vertex_count)
    }

    /// Its range in [`MeshletMesh::triangles`] (3 bytes per triangle).
    pub fn triangle_range(&self) -> Range<usize> {
        let start = self.triangle_offset as usize;
        start..start + 3 * usize::from(self.triangle_count)
    }
}

/// A mesh split into meshlets.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshletMesh {
    /// Meshlets, grouped by material in ascending order.
    pub meshlets: Vec<Meshlet>,
    /// Vertex references into the source mesh.
    pub vertices: Vec<u32>,
    /// Local corners (indices into a meshlet's vertex range), 3 per triangle; each meshlet's
    /// run starts 4-byte aligned (zero padding between runs).
    pub triangles: Vec<u8>,
    /// Triangles in the source mesh.
    pub source_triangles: u32,
    /// Vertices in the source mesh (every vertex reference is below it).
    pub source_vertices: u32,
    /// The settings it was built with.
    pub settings: MeshletBuildSettings,
}

impl MeshletMesh {
    /// Meshlet `m`'s triangles as source vertex indices, in stored winding.
    pub fn triangles_of(&self, m: &Meshlet) -> impl Iterator<Item = [u32; 3]> + '_ {
        let verts = &self.vertices[m.vertex_range()];
        self.triangles[m.triangle_range()]
            .as_chunks::<3>()
            .0
            .iter()
            .map(move |c| c.map(|i| verts[usize::from(i)]))
    }
}
