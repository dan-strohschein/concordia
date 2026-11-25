//! Build settings and the per-device meshlet size.

use crate::ClusterError;

/// Triangles per meshlet (and the default vertex cap).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MeshletSize {
    /// 64 triangles.
    S64,
    /// 128 triangles: the portable default (Metal 256/512, `VK_EXT_mesh_shader` ≥ 256/256).
    S128,
}

impl MeshletSize {
    /// The triangle count.
    pub fn triangles(self) -> u32 {
        match self {
            Self::S64 => 64,
            Self::S128 => 128,
        }
    }

    /// The largest size a device's mesh-output limits take; any size without mesh shaders.
    pub fn for_limits(
        mesh_shaders: bool,
        max_vertices: u32,
        max_primitives: u32,
    ) -> Result<Self, ClusterError> {
        if !mesh_shaders {
            return Ok(Self::S128);
        }
        [Self::S128, Self::S64]
            .into_iter()
            .find(|s| max_vertices >= s.triangles() && max_primitives >= s.triangles())
            .ok_or(ClusterError::Limits {
                vertices: max_vertices,
                primitives: max_primitives,
            })
    }
}

/// How meshlets are built; validated on construction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshletBuildSettings {
    size: MeshletSize,
    max_vertices: u32,
    cone_weight: f32,
}

impl MeshletBuildSettings {
    /// Settings with `max_vertices` in 64..=128 and `cone_weight` in 0..=1.
    pub fn new(
        size: MeshletSize,
        max_vertices: u32,
        cone_weight: f32,
    ) -> Result<Self, ClusterError> {
        if !(64..=128).contains(&max_vertices) {
            return Err(ClusterError::Settings(format!(
                "max_vertices {max_vertices} is outside 64..=128"
            )));
        }
        if !(0.0..=1.0).contains(&cone_weight) {
            return Err(ClusterError::Settings(format!(
                "cone_weight {cone_weight} is outside 0..=1"
            )));
        }
        Ok(Self {
            size,
            max_vertices,
            cone_weight,
        })
    }

    /// The default for `size`: as many vertices as triangles, cone weight 0.25.
    pub fn for_size(size: MeshletSize) -> Self {
        Self {
            size,
            max_vertices: size.triangles(),
            cone_weight: 0.25,
        }
    }

    /// Triangles per meshlet.
    pub fn size(&self) -> MeshletSize {
        self.size
    }

    /// Vertices per meshlet.
    pub fn max_vertices(&self) -> u32 {
        self.max_vertices
    }

    /// meshoptimizer's balance between cluster size and cone tightness.
    pub fn cone_weight(&self) -> f32 {
        self.cone_weight
    }
}

impl Default for MeshletBuildSettings {
    fn default() -> Self {
        Self::for_size(MeshletSize::S128)
    }
}
