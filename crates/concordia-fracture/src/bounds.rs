//! A mesh's axis-aligned bounds.

use manifold_sys::safe::Mesh;

use crate::FractureError;

/// An axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// The lowest corner.
    pub min: [f32; 3],
    /// The highest corner.
    pub max: [f32; 3],
}

impl Bounds {
    /// The bounds of a mesh's vertices, after checking the mesh is well formed.
    pub fn of(mesh: &Mesh) -> Result<Self, FractureError> {
        let invalid = |why: &str| Err(FractureError::InvalidMesh(why.into()));
        if mesh.positions.is_empty() || mesh.triangles.is_empty() {
            return invalid("the mesh is empty");
        }
        if mesh.positions.iter().flatten().any(|v| !v.is_finite()) {
            return invalid("a position isn't finite");
        }
        let n = mesh.positions.len();
        if mesh.triangles.iter().flatten().any(|&i| i as usize >= n) {
            return invalid("a triangle index is out of range");
        }
        let mut b = Self {
            min: mesh.positions[0],
            max: mesh.positions[0],
        };
        for p in &mesh.positions {
            for a in 0..3 {
                b.min[a] = b.min[a].min(p[a]);
                b.max[a] = b.max[a].max(p[a]);
            }
        }
        Ok(b)
    }

    /// The box's size per axis.
    pub fn size(&self) -> [f32; 3] {
        [0, 1, 2].map(|a| self.max[a] - self.min[a])
    }

    /// Half the diagonal's length.
    pub fn radius(&self) -> f32 {
        let s = self.size();
        (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt() * 0.5
    }
}
