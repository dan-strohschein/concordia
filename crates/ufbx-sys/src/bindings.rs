// Hand-written bindings for `shim/concordia_ufbx.h`. `tests/smoke.rs` checks the header's blake3
// and every struct's size against the C compiler's; update both when the header changes.
// header blake3: 3b6a642dd23655dcb4107b05224303873b9893906ef99a013e9e10dacb4d5fd8

use std::os::raw::c_char;

/// "No index" for node, mesh, skin and texture fields.
pub const CU_NONE: i32 = -1;
/// Baked translation keys.
pub const CU_CHANNEL_TRANSLATION: u32 = 0;
/// Baked rotation keys.
pub const CU_CHANNEL_ROTATION: u32 = 1;
/// Baked scale keys.
pub const CU_CHANNEL_SCALE: u32 = 2;

/// An opaque loaded scene.
#[repr(C)]
pub struct cu_scene {
    _private: [u8; 0],
}

/// An opaque baked animation stack.
#[repr(C)]
pub struct cu_bake {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cu_str {
    pub data: *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cu_bytes {
    pub data: *const u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cu_settings {
    pub original_unit_meters: f64,
    pub frames_per_second: f64,
    pub original_up_axis: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cu_node {
    pub name: cu_str,
    pub parent: i32,
    pub mesh: i32,
    pub num_materials: u32,
    pub is_root: bool,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cu_mesh {
    pub name: cu_str,
    pub num_parts: u32,
    pub num_vertices: u32,
    pub skin: i32,
    pub has_normals: bool,
    pub has_uvs: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct cu_corner {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub vertex: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct cu_skin {
    pub num_clusters: u32,
    pub max_weights: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct cu_cluster {
    pub bone: i32,
    pub geometry_to_bone: [f32; 16],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct cu_weight {
    pub cluster: u32,
    pub weight: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cu_material {
    pub name: cu_str,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub emissive_strength: f32,
    pub base_color_texture: i32,
    pub normal_texture: i32,
    pub emissive_texture: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cu_texture_file {
    pub relative_filename: cu_str,
    pub content: cu_bytes,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct cu_stack {
    pub name: cu_str,
    pub time_begin: f64,
    pub time_end: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct cu_baked_node {
    pub node: i32,
    pub num_keys: [u32; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct cu_key {
    pub time: f32,
    pub value: [f32; 4],
}

unsafe extern "C" {
    pub fn cu_struct_size(which: u32) -> usize;
    pub fn cu_load(data: *const u8, len: usize, error: *mut c_char, error_len: usize) -> *mut cu_scene;
    pub fn cu_free(scene: *mut cu_scene);
    pub fn cu_settings_get(scene: *const cu_scene, out: *mut cu_settings) -> bool;
    pub fn cu_node_count(scene: *const cu_scene) -> usize;
    pub fn cu_node_get(scene: *const cu_scene, node: usize, out: *mut cu_node) -> bool;
    pub fn cu_node_material(scene: *const cu_scene, node: usize, slot: usize) -> i32;
    pub fn cu_mesh_count(scene: *const cu_scene) -> usize;
    pub fn cu_mesh_get(scene: *const cu_scene, mesh: usize, out: *mut cu_mesh) -> bool;
    pub fn cu_mesh_part_triangles(scene: *const cu_scene, mesh: usize, part: usize) -> usize;
    pub fn cu_mesh_part_corners(
        scene: *const cu_scene,
        mesh: usize,
        part: usize,
        out: *mut cu_corner,
        cap: usize,
    ) -> usize;
    pub fn cu_skin_get(scene: *const cu_scene, skin: usize, out: *mut cu_skin) -> bool;
    pub fn cu_skin_cluster(
        scene: *const cu_scene,
        skin: usize,
        cluster: usize,
        out: *mut cu_cluster,
    ) -> bool;
    pub fn cu_skin_weights(
        scene: *const cu_scene,
        skin: usize,
        vertex: usize,
        out: *mut cu_weight,
        cap: usize,
    ) -> usize;
    pub fn cu_material_count(scene: *const cu_scene) -> usize;
    pub fn cu_material_get(scene: *const cu_scene, material: usize, out: *mut cu_material) -> bool;
    pub fn cu_texture_file_count(scene: *const cu_scene) -> usize;
    pub fn cu_texture_file_get(
        scene: *const cu_scene,
        file: usize,
        out: *mut cu_texture_file,
    ) -> bool;
    pub fn cu_stack_count(scene: *const cu_scene) -> usize;
    pub fn cu_stack_get(scene: *const cu_scene, stack: usize, out: *mut cu_stack) -> bool;
    pub fn cu_bake_stack(
        scene: *const cu_scene,
        stack: usize,
        rate: f64,
        error: *mut c_char,
        error_len: usize,
    ) -> *mut cu_bake;
    pub fn cu_bake_free(bake: *mut cu_bake);
    pub fn cu_bake_duration(bake: *const cu_bake) -> f64;
    pub fn cu_bake_node_count(bake: *const cu_bake) -> usize;
    pub fn cu_bake_node_get(bake: *const cu_bake, node: usize, out: *mut cu_baked_node) -> bool;
    pub fn cu_bake_keys(
        bake: *const cu_bake,
        node: usize,
        channel: u32,
        out: *mut cu_key,
        cap: usize,
    ) -> usize;
}
