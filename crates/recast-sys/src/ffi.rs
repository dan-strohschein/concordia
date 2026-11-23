//! Hand-written bindings mirroring `shim/concordia_recast.h`.
// header blake3: fd52a4c1998fd197c945491fb0776ddf9b96901438d24d77adc8d0361dd422ad

#![allow(non_camel_case_types, missing_docs)]

pub const CR_OK: i32 = 0;
pub const CR_EMPTY: i32 = 1;
pub const CR_ERR_NOT_FOUND: i32 = -21;
pub const CR_AREA_BLOCKED: u8 = 1;
pub const CR_AREA_WALKABLE: u8 = 63;
pub const CR_MAX_AREAS: usize = 64;
pub const CR_MAX_VERTS_PER_POLY: usize = 6;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cr_config {
    pub cell_size: f32,
    pub cell_height: f32,
    pub agent_height: f32,
    pub agent_radius: f32,
    pub agent_max_climb: f32,
    pub agent_max_slope_deg: f32,
    pub tile_size: i32,
    pub region_min_size: i32,
    pub region_merge_size: i32,
    pub edge_max_len: f32,
    pub edge_max_error: f32,
    pub verts_per_poly: i32,
    pub detail_sample_dist: f32,
    pub detail_sample_max_error: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cr_box_mark {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub area: u8,
    pub pad: [u8; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cr_tile_input {
    pub verts: *const f32,
    pub tris: *const i32,
    pub marks: *const cr_box_mark,
    pub vert_count: i32,
    pub tri_count: i32,
    pub mark_count: i32,
    pub tile_x: i32,
    pub tile_y: i32,
    pub bmin: [f32; 3],
    pub bmax: [f32; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cr_poly {
    pub r#ref: u64,
    pub verts: [f32; CR_MAX_VERTS_PER_POLY * 3],
    pub vert_count: u8,
    pub area: u8,
    pub pad: [u8; 6],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cr_filter {
    pub area_cost: [f32; CR_MAX_AREAS],
}

#[repr(C)]
pub struct cr_navmesh {
    _private: [u8; 0],
}

#[repr(C)]
pub struct cr_query {
    _private: [u8; 0],
}

unsafe extern "C" {
    pub fn cr_live_allocations() -> i64;
    pub fn cr_struct_size(which: i32) -> i32;
    pub fn cr_build_tile(
        config: *const cr_config,
        input: *const cr_tile_input,
        out_data: *mut *mut u8,
        out_size: *mut i32,
    ) -> i32;
    pub fn cr_free(data: *mut core::ffi::c_void);
    pub fn cr_navmesh_create(
        origin: *const f32,
        tile_width: f32,
        tile_height: f32,
        max_tiles: i32,
        max_polys_per_tile: i32,
    ) -> *mut cr_navmesh;
    pub fn cr_navmesh_free(mesh: *mut cr_navmesh);
    pub fn cr_navmesh_add_tile(mesh: *mut cr_navmesh, data: *const u8, size: i32) -> i32;
    pub fn cr_navmesh_tile_slots(mesh: *const cr_navmesh) -> i32;
    pub fn cr_navmesh_tile_info(
        mesh: *const cr_navmesh,
        slot: i32,
        tile_x: *mut i32,
        tile_y: *mut i32,
        poly_count: *mut i32,
    ) -> i32;
    pub fn cr_navmesh_poly(
        mesh: *const cr_navmesh,
        slot: i32,
        index: i32,
        out: *mut cr_poly,
    ) -> i32;
    pub fn cr_navmesh_poly_links(
        mesh: *const cr_navmesh,
        r#ref: u64,
        out: *mut u64,
        cap: i32,
    ) -> i32;
    pub fn cr_query_create(mesh: *const cr_navmesh, max_nodes: i32) -> *mut cr_query;
    pub fn cr_query_free(query: *mut cr_query);
    pub fn cr_find_nearest(
        query: *mut cr_query,
        filter: *const cr_filter,
        center: *const f32,
        half_extents: *const f32,
        out_ref: *mut u64,
        out_point: *mut f32,
    ) -> i32;
    pub fn cr_find_path(
        query: *mut cr_query,
        filter: *const cr_filter,
        start_ref: u64,
        end_ref: u64,
        start: *const f32,
        end: *const f32,
        path: *mut u64,
        cap: i32,
        count: *mut i32,
        partial: *mut u8,
    ) -> i32;
    pub fn cr_straight_path(
        query: *mut cr_query,
        start: *const f32,
        end: *const f32,
        path: *const u64,
        path_count: i32,
        points: *mut f32,
        cap: i32,
        count: *mut i32,
    ) -> i32;
    pub fn cr_raycast(
        query: *mut cr_query,
        filter: *const cr_filter,
        start_ref: u64,
        start: *const f32,
        end: *const f32,
        t: *mut f32,
        normal: *mut f32,
    ) -> i32;
    pub fn cr_move_along_surface(
        query: *mut cr_query,
        filter: *const cr_filter,
        start_ref: u64,
        start: *const f32,
        end: *const f32,
        out_point: *mut f32,
        visited: *mut u64,
        cap: i32,
        count: *mut i32,
    ) -> i32;
    pub fn cr_poly_height(
        query: *mut cr_query,
        r#ref: u64,
        pos: *const f32,
        height: *mut f32,
    ) -> i32;
    pub fn cr_raycast_path(
        query: *mut cr_query,
        filter: *const cr_filter,
        start_ref: u64,
        start: *const f32,
        end: *const f32,
        t: *mut f32,
        normal: *mut f32,
        path: *mut u64,
        cap: i32,
        count: *mut i32,
    ) -> i32;
    pub fn cr_local_neighbourhood(
        query: *mut cr_query,
        filter: *const cr_filter,
        start_ref: u64,
        center: *const f32,
        radius: f32,
        refs: *mut u64,
        cap: i32,
        count: *mut i32,
    ) -> i32;
    pub fn cr_poly_wall_segments(
        query: *mut cr_query,
        filter: *const cr_filter,
        r#ref: u64,
        segments: *mut f32,
        cap: i32,
        count: *mut i32,
    ) -> i32;
}
