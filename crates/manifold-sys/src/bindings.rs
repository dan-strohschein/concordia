// header blake3: a15309b91a94457e5c06f7f3068946ee8c58b66d3a1411f9bb0a5563205a96e7
// Hand-written from shim/concordia_manifold.h; `header_hash_matches_bindings` keeps them in step.

/// An opaque clipped piece.
#[repr(C)]
pub struct CmPiece {
    _private: [u8; 0],
}

/// `CmStatus`.
pub type CmStatus = u32;
pub const CM_OK: CmStatus = 0;
pub const CM_INVALID_INPUT: CmStatus = 1;
pub const CM_NOT_MANIFOLD: CmStatus = 2;
pub const CM_FAILED: CmStatus = 3;

unsafe extern "C" {
    pub fn cm_clip_convex(
        positions: *const f32,
        vertex_count: u32,
        triangles: *const u32,
        triangle_count: u32,
        planes: *const f32,
        plane_count: u32,
        out: *mut *mut CmPiece,
    ) -> CmStatus;
    pub fn cm_piece_vertex_count(piece: *const CmPiece) -> u32;
    pub fn cm_piece_triangle_count(piece: *const CmPiece) -> u32;
    pub fn cm_piece_volume(piece: *const CmPiece) -> f64;
    pub fn cm_piece_copy(
        piece: *const CmPiece,
        positions: *mut f32,
        triangles: *mut u32,
        interior: *mut u8,
    );
    pub fn cm_piece_free(piece: *mut CmPiece);
}
