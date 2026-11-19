//! Camera frustum culling: which meshes can appear in the image.

use concordia_core::math::{Mat4, Vec3, Vec4};

/// The six planes of a view-projection (any projection; reverse-Z, depth in [0, 1]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frustum([Vec4; 6]);

impl Frustum {
    /// Planes from `view_proj` (Gribb–Hartmann), facing inward.
    pub(crate) fn new(view_proj: Mat4) -> Self {
        let rows = [
            view_proj.row(0),
            view_proj.row(1),
            view_proj.row(2),
            view_proj.row(3),
        ];
        let [r0, r1, r2, r3] = rows;
        Self([r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2])
    }

    /// Whether the box `lo..hi`, transformed by `world`, may be inside (conservative).
    pub(crate) fn sees(&self, world: &Mat4, lo: Vec3, hi: Vec3) -> bool {
        let corners = (0..8).map(|i| {
            let pick = |bit: usize, a: f32, b: f32| if i & bit == 0 { a } else { b };
            world.transform_point3(Vec3::new(
                pick(1, lo.x, hi.x),
                pick(2, lo.y, hi.y),
                pick(4, lo.z, hi.z),
            ))
        });
        let corners: Vec<Vec3> = corners.collect();
        self.0
            .iter()
            .all(|plane| corners.iter().any(|c| plane.dot(c.extend(1.0)) >= 0.0))
    }
}
