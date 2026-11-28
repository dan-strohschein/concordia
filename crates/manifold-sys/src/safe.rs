//! The checked wrapper: input is validated before Manifold runs, and every result is copied out.

use crate::ManifoldError;
use crate::ffi;

/// A closed triangle mesh.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    /// Vertex positions.
    pub positions: Vec<[f32; 3]>,
    /// Triangles, counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
}

/// The half-space `normal · x ≤ offset` (the normal need not be unit length).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// The outward normal.
    pub normal: [f32; 3],
    /// The plane's offset along the normal.
    pub offset: f32,
}

/// A clipped piece.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Piece {
    /// Vertex positions.
    pub positions: Vec<[f32; 3]>,
    /// Triangles.
    pub triangles: Vec<[u32; 3]>,
    /// Per triangle: it lies on a cut (an interior face).
    pub interior: Vec<bool>,
    /// Its volume.
    pub volume: f64,
}

/// `mesh` clipped by the convex cell where every plane's `normal · x ≤ offset` holds.
pub fn clip_convex(mesh: &Mesh, planes: &[Plane]) -> Result<Piece, ManifoldError> {
    let count = |n: usize, what: &str| {
        u32::try_from(n).map_err(|_| ManifoldError::Invalid(format!("too many {what}")))
    };
    let (vertices, triangles) = (
        count(mesh.positions.len(), "vertices")?,
        count(mesh.triangles.len(), "triangles")?,
    );
    let flat_planes: Vec<f32> = planes
        .iter()
        .flat_map(|p| [p.normal[0], p.normal[1], p.normal[2], p.offset])
        .collect();
    let mut out: *mut ffi::CmPiece = std::ptr::null_mut();
    // SAFETY: the arrays hold 3 floats per vertex, 3 indices per triangle and 4 floats per plane,
    // with the counts given; `out` is written only on success.
    let status = unsafe {
        ffi::cm_clip_convex(
            mesh.positions.as_ptr().cast(),
            vertices,
            mesh.triangles.as_ptr().cast(),
            triangles,
            flat_planes.as_ptr(),
            count(planes.len(), "planes")?,
            &mut out,
        )
    };
    match status {
        ffi::CM_OK if !out.is_null() => Ok(copy_out(out)),
        ffi::CM_NOT_MANIFOLD => Err(ManifoldError::NotManifold),
        ffi::CM_INVALID_INPUT => Err(ManifoldError::Invalid(
            "empty mesh, index out of range, value not finite or zero normal".into(),
        )),
        _ => Err(ManifoldError::Failed),
    }
}

/// Copies and frees a piece.
fn copy_out(raw: *mut ffi::CmPiece) -> Piece {
    // SAFETY: `raw` is a live piece from `cm_clip_convex`; the buffers have the sizes it reports;
    // it is freed exactly once, here.
    unsafe {
        let v = ffi::cm_piece_vertex_count(raw) as usize;
        let t = ffi::cm_piece_triangle_count(raw) as usize;
        let mut positions = vec![[0.0f32; 3]; v];
        let mut triangles = vec![[0u32; 3]; t];
        let mut interior = vec![0u8; t];
        ffi::cm_piece_copy(
            raw,
            positions.as_mut_ptr().cast(),
            triangles.as_mut_ptr().cast(),
            interior.as_mut_ptr(),
        );
        let volume = ffi::cm_piece_volume(raw);
        ffi::cm_piece_free(raw);
        Piece {
            positions,
            triangles,
            interior: interior.into_iter().map(|i| i != 0).collect(),
            volume,
        }
    }
}
