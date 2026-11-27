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

/// A simplified triangle list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Simplified {
    /// The kept triangles' corners (into the same positions).
    pub indices: Vec<u32>,
    /// The error reached, relative to the mesh's extent.
    pub error: f32,
}

/// Simplifies a triangle list toward `target_index_count` corners (0: as few as the error
/// allows) without moving its surface more than `target_error` of its extent
/// (`meshopt_simplify`).
pub fn simplify(
    indices: &[u32],
    positions: &[[f32; 3]],
    target_index_count: usize,
    target_error: f32,
) -> Result<Simplified, MeshoptError> {
    if !(0.0..=1.0).contains(&target_error) {
        return Err(MeshoptError::TargetError(target_error));
    }
    if !indices.len().is_multiple_of(3) {
        return Err(MeshoptError::IndexCount(indices.len()));
    }
    check_refs(indices, positions)?;
    // It scales by every vertex's extent, referenced or not.
    if let Some(bad) = positions
        .iter()
        .position(|p| !p.iter().all(|c| c.is_finite()))
    {
        return Err(MeshoptError::NonFinitePosition(bad));
    }
    if indices.is_empty() {
        return Ok(Simplified::default());
    }
    let mut out = vec![0u32; indices.len()];
    let mut error = 0.0f32;
    // SAFETY: `out` holds index_count entries (the documented worst case); indices are in range
    // and the positions they reference finite; no option bits are set.
    let count = unsafe {
        ffi::meshopt_simplify(
            out.as_mut_ptr(),
            indices.as_ptr(),
            indices.len(),
            positions.as_ptr().cast(),
            positions.len(),
            STRIDE,
            target_index_count.min(indices.len()),
            target_error,
            0,
            &mut error,
        )
    };
    debug_assert!(count <= indices.len() && count.is_multiple_of(3));
    out.truncate(count);
    Ok(Simplified {
        indices: out,
        error,
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

/// Every reference points at a finite position (only referenced positions are read).
fn check_refs(indices: &[u32], positions: &[[f32; 3]]) -> Result<(), MeshoptError> {
    for &index in indices {
        let Some(p) = positions.get(index as usize) else {
            return Err(MeshoptError::IndexRange {
                index,
                vertices: positions.len(),
            });
        };
        if !p.iter().all(|c| c.is_finite()) {
            return Err(MeshoptError::NonFinitePosition(index as usize));
        }
    }
    Ok(())
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
