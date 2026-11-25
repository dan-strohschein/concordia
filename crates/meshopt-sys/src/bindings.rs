// Hand-written mirror of the parts of meshoptimizer.h that Concordia uses (v1.3).
// header blake3: f081362dd20f3b0d551f2655d4e658f8ff96e795289d4b65f162d026d91fc9a9

use std::os::raw::{c_float, c_int, c_uint};

/// `struct meshopt_Meshlet`: offsets into the shared vertex and triangle arrays, and counts.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct meshopt_Meshlet {
    pub vertex_offset: c_uint,
    pub triangle_offset: c_uint,
    pub vertex_count: c_uint,
    pub triangle_count: c_uint,
}

/// `struct meshopt_Bounds`: bounding sphere and normal cone (float and 8-bit SNORM).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct meshopt_Bounds {
    pub center: [c_float; 3],
    pub radius: c_float,
    pub cone_apex: [c_float; 3],
    pub cone_axis: [c_float; 3],
    pub cone_cutoff: c_float,
    pub cone_axis_s8: [i8; 3],
    pub cone_cutoff_s8: i8,
}

unsafe extern "C" {
    pub fn meshopt_buildMeshlets(
        meshlets: *mut meshopt_Meshlet,
        meshlet_vertices: *mut c_uint,
        meshlet_triangles: *mut u8,
        indices: *const c_uint,
        index_count: usize,
        vertex_positions: *const c_float,
        vertex_count: usize,
        vertex_positions_stride: usize,
        max_vertices: usize,
        max_triangles: usize,
        cone_weight: c_float,
    ) -> usize;
    pub fn meshopt_buildMeshletsBound(
        index_count: usize,
        max_vertices: usize,
        max_triangles: usize,
    ) -> usize;
    pub fn meshopt_optimizeMeshlet(
        meshlet_vertices: *mut c_uint,
        meshlet_triangles: *mut u8,
        triangle_count: usize,
        vertex_count: usize,
    );
    pub fn meshopt_computeMeshletBounds(
        meshlet_vertices: *const c_uint,
        meshlet_triangles: *const u8,
        triangle_count: usize,
        vertex_positions: *const c_float,
        vertex_count: usize,
        vertex_positions_stride: usize,
    ) -> meshopt_Bounds;

    /// Concordia's shim: `sizeof` of struct `which` (0 Meshlet, 1 Bounds), 0 past the end.
    pub fn cm_struct_size(which: c_uint) -> usize;
    /// Concordia's shim: `MESHOPTIMIZER_VERSION` (1030 for 1.3).
    pub fn cm_version() -> c_int;
}
