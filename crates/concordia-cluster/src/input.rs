//! The mesh a build reads: positions, a triangle list and per-triangle materials.

use crate::ClusterError;

/// A validated triangle mesh to split into meshlets.
#[derive(Debug, Clone, Copy)]
pub struct ClusterInput<'a> {
    positions: &'a [[f32; 3]],
    indices: &'a [u32],
    materials: Option<&'a [u16]>,
}

impl<'a> ClusterInput<'a> {
    /// Checks the triangle list, index range, finite positions and one material per triangle
    /// (`None`: every triangle is material 0).
    pub fn new(
        positions: &'a [[f32; 3]],
        indices: &'a [u32],
        materials: Option<&'a [u16]>,
    ) -> Result<Self, ClusterError> {
        let fail = |m: String| Err(ClusterError::Input(m));
        if !indices.len().is_multiple_of(3) {
            return fail(format!("{} indices is not a triangle list", indices.len()));
        }
        if let Some(i) = indices.iter().find(|&&i| i as usize >= positions.len()) {
            return fail(format!("index {i} is past {} positions", positions.len()));
        }
        if let Some(p) = positions
            .iter()
            .position(|p| !p.iter().all(|c| c.is_finite()))
        {
            return fail(format!("position {p} is not finite"));
        }
        if let Some(m) = materials.filter(|m| m.len() != indices.len() / 3) {
            return fail(format!(
                "{} materials for {} triangles",
                m.len(),
                indices.len() / 3
            ));
        }
        Ok(Self {
            positions,
            indices,
            materials,
        })
    }

    /// Positions.
    pub fn positions(&self) -> &'a [[f32; 3]] {
        self.positions
    }

    /// The triangle list.
    pub fn indices(&self) -> &'a [u32] {
        self.indices
    }

    /// Triangle `t`'s material.
    pub fn material(&self, t: usize) -> u16 {
        self.materials.map_or(0, |m| m[t])
    }

    /// The number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}
