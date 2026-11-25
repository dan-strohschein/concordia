//! The checked wrapper: every limit is validated before meshoptimizer runs.

use crate::MeshoptError;
use crate::ffi::{self, meshopt_Bounds, meshopt_Meshlet};

/// meshoptimizer's per-meshlet vertex limit.
pub const MAX_VERTICES: usize = 256;
/// meshoptimizer's per-meshlet triangle limit.
pub const MAX_TRIANGLES: usize = 512;
/// The byte stride of `[f32; 3]` positions.
const STRIDE: usize = size_of::<[f32; 3]>();

/// Meshlets in meshoptimizer's packed layout, trimmed to what was written.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meshlets {
    /// Per-meshlet offsets and counts.
    pub meshlets: Vec<meshopt_Meshlet>,
    /// Vertex references into the mesh (indexed by `vertex_offset`).
    pub vertices: Vec<u32>,
    /// Local triangle corners, 3 per triangle (indexed by `triangle_offset`).
    pub triangles: Vec<u8>,
}

/// Splits a triangle list into meshlets (`meshopt_buildMeshlets`).
pub fn build_meshlets(
    indices: &[u32],
    positions: &[[f32; 3]],
    max_vertices: usize,
    max_triangles: usize,
    cone_weight: f32,
) -> Result<Meshlets, MeshoptError> {
    check_limits(max_vertices, max_triangles, cone_weight)?;
    if !indices.len().is_multiple_of(3) {
        return Err(MeshoptError::IndexCount(indices.len()));
    }
    check_refs(indices, positions)?;
    if indices.is_empty() {
        return Ok(Meshlets::default());
    }
    // SAFETY: pure arithmetic on checked limits.
    let bound =
        unsafe { ffi::meshopt_buildMeshletsBound(indices.len(), max_vertices, max_triangles) };
    let mut meshlets = vec![meshopt_Meshlet::default(); bound];
    let mut vertices = vec![0u32; indices.len()];
    let mut triangles = vec![0u8; indices.len()];
    // SAFETY: the output buffers have the documented worst-case sizes (the bound, and
    // index_count each); indices are in range and positions finite, so no assertion fires.
    let count = unsafe {
        ffi::meshopt_buildMeshlets(
            meshlets.as_mut_ptr(),
            vertices.as_mut_ptr(),
            triangles.as_mut_ptr(),
            indices.as_ptr(),
            indices.len(),
            positions.as_ptr().cast(),
            positions.len(),
            STRIDE,
            max_vertices,
            max_triangles,
            cone_weight,
        )
    };
    debug_assert!(count <= bound);
    meshlets.truncate(count);
    if let Some(last) = meshlets.last() {
        vertices.truncate((last.vertex_offset + last.vertex_count) as usize);
        triangles.truncate((last.triangle_offset + last.triangle_count * 3) as usize);
    } else {
        vertices.clear();
        triangles.clear();
    }
    Ok(Meshlets {
        meshlets,
        vertices,
        triangles,
    })
}

/// Reorders one meshlet's vertices and triangles for locality (`meshopt_optimizeMeshlet`).
pub fn optimize_meshlet(vertices: &mut [u32], triangles: &mut [u8]) -> Result<(), MeshoptError> {
    check_meshlet(vertices, triangles)?;
    // SAFETY: counts are within meshoptimizer's limits and every corner is a valid local
    // vertex, so it reads and writes only inside the two slices.
    unsafe {
        ffi::meshopt_optimizeMeshlet(
            vertices.as_mut_ptr(),
            triangles.as_mut_ptr(),
            triangles.len() / 3,
            vertices.len(),
        );
    }
    Ok(())
}

/// One meshlet's bounding sphere and normal cone (`meshopt_computeMeshletBounds`).
pub fn meshlet_bounds(
    vertices: &[u32],
    triangles: &[u8],
    positions: &[[f32; 3]],
) -> Result<meshopt_Bounds, MeshoptError> {
    check_meshlet(vertices, triangles)?;
    check_refs(vertices, positions)?;
    // SAFETY: local corners index `vertices`, whose entries index `positions` (checked), and
    // the triangle count is within the limit.
    Ok(unsafe {
        ffi::meshopt_computeMeshletBounds(
            vertices.as_ptr(),
            triangles.as_ptr(),
            triangles.len() / 3,
            positions.as_ptr().cast(),
            positions.len(),
            STRIDE,
        )
    })
}

fn check_limits(
    max_vertices: usize,
    max_triangles: usize,
    cone_weight: f32,
) -> Result<(), MeshoptError> {
    if !(3..=MAX_VERTICES).contains(&max_vertices) {
        return Err(MeshoptError::MaxVertices(max_vertices));
    }
    if !(1..=MAX_TRIANGLES).contains(&max_triangles) {
        return Err(MeshoptError::MaxTriangles(max_triangles));
    }
    if !(0.0..=1.0).contains(&cone_weight) {
        return Err(MeshoptError::ConeWeight(cone_weight));
    }
    Ok(())
}

/// Every reference points at a position, and every position is finite.
fn check_refs(indices: &[u32], positions: &[[f32; 3]]) -> Result<(), MeshoptError> {
    if let Some(&index) = indices.iter().find(|&&i| i as usize >= positions.len()) {
        return Err(MeshoptError::IndexRange {
            index,
            vertices: positions.len(),
        });
    }
    match positions
        .iter()
        .position(|p| !p.iter().all(|c| c.is_finite()))
    {
        Some(i) => Err(MeshoptError::NonFinitePosition(i)),
        None => Ok(()),
    }
}

fn check_meshlet(vertices: &[u32], triangles: &[u8]) -> Result<(), MeshoptError> {
    if vertices.len() > MAX_VERTICES {
        return Err(MeshoptError::Meshlet(format!(
            "{} vertices exceed {MAX_VERTICES}",
            vertices.len()
        )));
    }
    if !triangles.len().is_multiple_of(3) || triangles.len() / 3 > MAX_TRIANGLES {
        return Err(MeshoptError::Meshlet(format!(
            "{} corners is not a triangle list of at most {MAX_TRIANGLES}",
            triangles.len()
        )));
    }
    match triangles.iter().find(|&&c| c as usize >= vertices.len()) {
        Some(c) => Err(MeshoptError::Meshlet(format!(
            "corner {c} is past {} vertices",
            vertices.len()
        ))),
        None => Ok(()),
    }
}
