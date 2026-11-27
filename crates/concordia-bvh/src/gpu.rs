//! The hierarchy flattened into the row buffers `shaders/bvh.slang` reads (16-byte rows).

use concordia_core::math::{Mat4, Vec3};

use crate::{Accel, Node, Tree};

/// Rows per instance in [`GpuBvh::instances`]: world → object (3), then `[mesh, 0, 0, 0]`.
pub const INSTANCE_ROWS: u32 = 4;
/// `tlas_node` when there are no instances.
pub const NO_TLAS: u32 = u32::MAX;

/// An [`Accel`] in the GPU layout; upload each buffer and pass their bindless indices.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpuBvh {
    /// Every tree's nodes (mesh trees, then the instance tree), 2 rows each: `[min, first]`,
    /// `[max, count]` (floats as bits).
    pub nodes: Vec<[u32; 4]>,
    /// Every tree's leaf order, concatenated.
    pub order: Vec<u32>,
    /// Every mesh's triangles, 3 rows each (corners, floats as bits).
    pub tris: Vec<[u32; 4]>,
    /// Per mesh: `[first node, first triangle, first order entry, 0]`.
    pub meshes: Vec<[u32; 4]>,
    /// Per instance, [`INSTANCE_ROWS`] rows.
    pub instances: Vec<[u32; 4]>,
    /// The instance tree's first node ([`NO_TLAS`] when empty).
    pub tlas_node: u32,
    /// The instance tree's first order entry.
    pub tlas_order: u32,
}

impl GpuBvh {
    /// Flattens `accel`.
    pub fn pack(accel: &Accel) -> Self {
        let mut out = Self::default();
        for mesh in accel.meshes() {
            out.meshes.push([
                (out.nodes.len() / 2) as u32,
                (out.tris.len() / 3) as u32,
                out.order.len() as u32,
                0,
            ]);
            for t in mesh.triangles() {
                out.tris.extend(t.v.map(|v| bits([v.x, v.y, v.z, 0.0])));
            }
            out.push_tree(mesh.nodes(), mesh.order());
        }
        for (instance, inverse) in accel.instances().iter().zip(accel.inverses()) {
            out.instances.extend((0..3).map(|r| bits(row(inverse, r))));
            out.instances.push([instance.mesh, 0, 0, 0]);
        }
        let tree: &Tree = accel.tree();
        (out.tlas_node, out.tlas_order) = if tree.nodes().is_empty() {
            (NO_TLAS, 0)
        } else {
            ((out.nodes.len() / 2) as u32, out.order.len() as u32)
        };
        out.push_tree(tree.nodes(), tree.order());
        out
    }

    fn push_tree(&mut self, nodes: &[Node], order: &[u32]) {
        for n in nodes {
            self.nodes.push(corner(n.min, n.first));
            self.nodes.push(corner(n.max, n.count));
        }
        self.order.extend_from_slice(order);
    }
}

fn corner(p: Vec3, word: u32) -> [u32; 4] {
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits(), word]
}

fn bits(v: [f32; 4]) -> [u32; 4] {
    v.map(f32::to_bits)
}

/// Row `r` of `m` (so `dot(row, (p, 1))` is that coordinate of `m · p`).
fn row(m: &Mat4, r: usize) -> [f32; 4] {
    [m.x_axis[r], m.y_axis[r], m.z_axis[r], m.w_axis[r]]
}
