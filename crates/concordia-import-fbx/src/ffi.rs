//! Safe wrapper over `ufbx-sys`: the crate's only `unsafe`. Every pointer the shim returns is
//! checked for null and copied into owned values while the scene is alive.
#![allow(unsafe_code)]

use std::marker::PhantomData;
use std::os::raw::c_char;
use std::ptr::NonNull;

use ufbx_sys as u;

const ERROR_LEN: usize = 512;

pub(crate) use u::{cu_corner as Corner, cu_key as Key};

/// One node (the implicit root has `is_root`).
pub(crate) struct Node {
    pub(crate) name: String,
    pub(crate) parent: Option<usize>,
    pub(crate) mesh: Option<usize>,
    pub(crate) materials: Vec<Option<usize>>,
    pub(crate) is_root: bool,
    pub(crate) translation: [f32; 3],
    pub(crate) rotation: [f32; 4],
    pub(crate) scale: [f32; 3],
}

/// One mesh: its name, triangles per material part, and skin.
pub(crate) struct Mesh {
    pub(crate) name: String,
    pub(crate) triangles: Vec<usize>,
    pub(crate) skin: Option<usize>,
    pub(crate) has_normals: bool,
    pub(crate) has_uvs: bool,
}

/// One skin cluster: the bone node and the geometry-to-bone (inverse bind) matrix.
pub(crate) struct Cluster {
    pub(crate) bone: usize,
    pub(crate) geometry_to_bone: [f32; 16],
}

/// A material's PBR values and texture files.
pub(crate) struct MaterialValues {
    pub(crate) name: String,
    pub(crate) base_color: [f32; 4],
    pub(crate) metallic: f32,
    pub(crate) roughness: f32,
    pub(crate) emissive: [f32; 3],
    pub(crate) emissive_strength: f32,
    pub(crate) base_color_texture: Option<usize>,
    pub(crate) normal_texture: Option<usize>,
    pub(crate) emissive_texture: Option<usize>,
}

/// An image a texture uses: its relative path and any embedded bytes.
pub(crate) struct TextureFile {
    pub(crate) relative_filename: String,
    pub(crate) content: Vec<u8>,
}

/// An animation stack.
pub(crate) struct Stack {
    pub(crate) name: String,
}

/// A loaded scene, borrowing the bytes it was read from.
pub(crate) struct Scene<'a> {
    raw: NonNull<u::cu_scene>,
    _bytes: PhantomData<&'a [u8]>,
}

fn index(i: i32) -> Option<usize> {
    usize::try_from(i).ok()
}

fn message(buffer: &[c_char]) -> String {
    let bytes: Vec<u8> = buffer
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| *c as u8)
        .collect();
    match String::from_utf8_lossy(&bytes).trim() {
        "" => "unknown error".to_owned(),
        m => m.to_owned(),
    }
}

fn refused(what: &str, i: usize) -> String {
    format!("ufbx refused {what} {i}")
}

/// Copies shim text; a null pointer is the empty string.
fn text(s: u::cu_str) -> String {
    if s.data.is_null() || s.len == 0 {
        return String::new();
    }
    // SAFETY: the shim returns (data, len) of a live scene's string; data is non-null.
    let bytes = unsafe { std::slice::from_raw_parts(s.data.cast::<u8>(), s.len) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// Copies shim bytes; a null pointer is empty.
fn bytes(b: u::cu_bytes) -> Vec<u8> {
    if b.data.is_null() || b.len == 0 {
        return Vec::new();
    }
    // SAFETY: the shim returns (data, len) of a live scene's blob; data is non-null.
    unsafe { std::slice::from_raw_parts(b.data, b.len) }.to_vec()
}

/// Calls a shim getter that fills `T`, or `None` when it refuses.
fn get<T: Copy>(fill: impl FnOnce(*mut T) -> bool) -> Option<T> {
    let mut out = std::mem::MaybeUninit::<T>::zeroed();
    // SAFETY: every shim struct is plain old data, so all-zero bytes are a valid value.
    fill(out.as_mut_ptr()).then(|| unsafe { out.assume_init() })
}

/// Fills a buffer of `len` items from a shim writer, keeping what it wrote.
fn fill_vec<T: Copy + Default>(len: usize, write: impl FnOnce(*mut T, usize) -> usize) -> Vec<T> {
    let mut out = vec![T::default(); len];
    let written = write(out.as_mut_ptr(), out.len());
    out.truncate(written.min(len));
    out
}

impl<'a> Scene<'a> {
    /// Loads FBX bytes (normalized to Y-up, right-handed meters) or returns ufbx's message.
    pub(crate) fn load(data: &'a [u8]) -> Result<Self, String> {
        let mut error = [0 as c_char; ERROR_LEN];
        // SAFETY: data and error are live buffers of the given lengths; the scene doesn't outlive data.
        let raw = unsafe { u::cu_load(data.as_ptr(), data.len(), error.as_mut_ptr(), error.len()) };
        NonNull::new(raw)
            .map(|raw| Self {
                raw,
                _bytes: PhantomData,
            })
            .ok_or_else(|| message(&error))
    }

    fn ptr(&self) -> *const u::cu_scene {
        self.raw.as_ptr()
    }

    /// The file's animation frame rate (0 when it declares none).
    pub(crate) fn frames_per_second(&self) -> f64 {
        // SAFETY: valid scene and out pointer.
        get(|out| unsafe { u::cu_settings_get(self.ptr(), out) })
            .map_or(0.0, |s: u::cu_settings| s.frames_per_second)
    }

    /// Every node, by index.
    pub(crate) fn nodes(&self) -> Result<Vec<Node>, String> {
        // SAFETY: valid scene.
        let count = unsafe { u::cu_node_count(self.ptr()) };
        (0..count)
            .map(|i| self.node(i).ok_or_else(|| refused("node", i)))
            .collect()
    }

    fn node(&self, i: usize) -> Option<Node> {
        // SAFETY: valid scene and out pointer; the shim checks the index.
        let n: u::cu_node = get(|out| unsafe { u::cu_node_get(self.ptr(), i, out) })?;
        let materials = (0..n.num_materials as usize)
            // SAFETY: valid scene; the shim checks both indices.
            .map(|slot| index(unsafe { u::cu_node_material(self.ptr(), i, slot) }))
            .collect();
        Some(Node {
            name: text(n.name),
            parent: index(n.parent),
            mesh: index(n.mesh),
            materials,
            is_root: n.is_root,
            translation: n.translation,
            rotation: n.rotation,
            scale: n.scale,
        })
    }

    /// Every mesh, by index.
    pub(crate) fn meshes(&self) -> Result<Vec<Mesh>, String> {
        // SAFETY: valid scene.
        let count = unsafe { u::cu_mesh_count(self.ptr()) };
        (0..count)
            .map(|i| {
                // SAFETY: valid scene and out pointer; the shim checks the index.
                let m: u::cu_mesh = get(|out| unsafe { u::cu_mesh_get(self.ptr(), i, out) })
                    .ok_or_else(|| refused("mesh", i))?;
                let triangles = (0..m.num_parts as usize)
                    // SAFETY: valid scene; the shim checks the indices.
                    .map(|p| unsafe { u::cu_mesh_part_triangles(self.ptr(), i, p) })
                    .collect();
                Ok(Mesh {
                    name: text(m.name),
                    triangles,
                    skin: index(m.skin),
                    has_normals: m.has_normals,
                    has_uvs: m.has_uvs,
                })
            })
            .collect()
    }

    /// A mesh part's triangulated corners (three per triangle).
    pub(crate) fn corners(&self, mesh: usize, part: usize, triangles: usize) -> Vec<Corner> {
        let len = triangles.saturating_mul(3);
        // SAFETY: the buffer holds `cap` corners; the shim writes at most that many.
        fill_vec(len, |out, cap| unsafe {
            u::cu_mesh_part_corners(self.ptr(), mesh, part, out, cap)
        })
    }

    /// A skin's clusters, in cluster order (`None` if any is broken).
    pub(crate) fn clusters(&self, skin: usize) -> Option<Vec<Cluster>> {
        // SAFETY: valid scene and out pointer; the shim checks the index.
        let info: u::cu_skin = get(|out| unsafe { u::cu_skin_get(self.ptr(), skin, out) })?;
        (0..info.num_clusters as usize)
            .map(|c| {
                // SAFETY: as above.
                let raw: u::cu_cluster =
                    get(|out| unsafe { u::cu_skin_cluster(self.ptr(), skin, c, out) })?;
                Some(Cluster {
                    bone: index(raw.bone)?,
                    geometry_to_bone: raw.geometry_to_bone,
                })
            })
            .collect()
    }

    /// A logical vertex's (cluster, weight) influences, largest first.
    pub(crate) fn weights(&self, skin: usize, vertex: u32) -> Vec<(u32, f32)> {
        let vertex = vertex as usize;
        // SAFETY: a null buffer with capacity 0 only asks for the count.
        let count =
            unsafe { u::cu_skin_weights(self.ptr(), skin, vertex, std::ptr::null_mut(), 0) };
        // SAFETY: the buffer holds `cap` weights; the shim writes at most that many.
        let written = fill_vec(count, |out, cap| unsafe {
            u::cu_skin_weights(self.ptr(), skin, vertex, out, cap).min(cap)
        });
        written
            .iter()
            .map(|w: &u::cu_weight| (w.cluster, w.weight))
            .collect()
    }

    /// Every material, by index.
    pub(crate) fn materials(&self) -> Result<Vec<MaterialValues>, String> {
        // SAFETY: valid scene.
        let count = unsafe { u::cu_material_count(self.ptr()) };
        (0..count)
            .map(|i| {
                // SAFETY: valid scene and out pointer; the shim checks the index.
                let m: u::cu_material =
                    get(|out| unsafe { u::cu_material_get(self.ptr(), i, out) })
                        .ok_or_else(|| refused("material", i))?;
                Ok(MaterialValues {
                    name: text(m.name),
                    base_color: m.base_color,
                    metallic: m.metallic,
                    roughness: m.roughness,
                    emissive: m.emissive,
                    emissive_strength: m.emissive_strength,
                    base_color_texture: index(m.base_color_texture),
                    normal_texture: index(m.normal_texture),
                    emissive_texture: index(m.emissive_texture),
                })
            })
            .collect()
    }

    /// Every image file textures refer to, by index.
    pub(crate) fn texture_files(&self) -> Result<Vec<TextureFile>, String> {
        // SAFETY: valid scene.
        let count = unsafe { u::cu_texture_file_count(self.ptr()) };
        (0..count)
            .map(|i| {
                // SAFETY: valid scene and out pointer; the shim checks the index.
                let f: u::cu_texture_file =
                    get(|out| unsafe { u::cu_texture_file_get(self.ptr(), i, out) })
                        .ok_or_else(|| refused("texture file", i))?;
                Ok(TextureFile {
                    relative_filename: text(f.relative_filename),
                    content: bytes(f.content),
                })
            })
            .collect()
    }

    /// Every animation stack, by index.
    pub(crate) fn stacks(&self) -> Result<Vec<Stack>, String> {
        // SAFETY: valid scene.
        let count = unsafe { u::cu_stack_count(self.ptr()) };
        (0..count)
            .map(|i| {
                // SAFETY: valid scene and out pointer; the shim checks the index.
                let s: u::cu_stack = get(|out| unsafe { u::cu_stack_get(self.ptr(), i, out) })
                    .ok_or_else(|| refused("animation stack", i))?;
                Ok(Stack { name: text(s.name) })
            })
            .collect()
    }

    /// Bakes a stack to linear keys from time 0 at `rate` keys per second.
    pub(crate) fn bake(&self, stack: usize, rate: f64) -> Result<Bake<'_>, String> {
        let mut error = [0 as c_char; ERROR_LEN];
        // SAFETY: valid scene and error buffer; the shim checks the index and rate.
        let raw =
            unsafe { u::cu_bake_stack(self.ptr(), stack, rate, error.as_mut_ptr(), error.len()) };
        NonNull::new(raw)
            .map(|raw| Bake {
                raw,
                _scene: PhantomData,
            })
            .ok_or_else(|| message(&error))
    }
}

impl Drop for Scene<'_> {
    fn drop(&mut self) {
        // SAFETY: the scene came from cu_load and is freed once.
        unsafe { u::cu_free(self.raw.as_ptr()) }
    }
}

/// One baked node: its slot in the bake, which scene node, and how many keys per channel.
pub(crate) struct BakedNode {
    pub(crate) slot: usize,
    pub(crate) node: usize,
    pub(crate) keys: [usize; 3],
}

/// A baked animation stack, alive no longer than its scene.
pub(crate) struct Bake<'s> {
    raw: NonNull<u::cu_bake>,
    _scene: PhantomData<&'s ()>,
}

impl Bake<'_> {
    fn ptr(&self) -> *const u::cu_bake {
        self.raw.as_ptr()
    }

    /// Playback length in seconds.
    pub(crate) fn duration(&self) -> f64 {
        // SAFETY: valid bake.
        unsafe { u::cu_bake_duration(self.ptr()) }
    }

    /// The animated nodes.
    pub(crate) fn nodes(&self) -> Vec<BakedNode> {
        // SAFETY: valid bake.
        let count = unsafe { u::cu_bake_node_count(self.ptr()) };
        (0..count)
            .filter_map(|i| {
                // SAFETY: valid bake and out pointer; the shim checks the index.
                let n: u::cu_baked_node =
                    get(|out| unsafe { u::cu_bake_node_get(self.ptr(), i, out) })?;
                Some(BakedNode {
                    slot: i,
                    node: index(n.node)?,
                    keys: n.num_keys.map(|k| k as usize),
                })
            })
            .collect()
    }

    /// One channel's keys (`CHANNELS`) of a baked node.
    pub(crate) fn keys(&self, node: &BakedNode, channel: usize) -> Vec<Key> {
        let (Some(id), Some(len)) = (CHANNELS.get(channel), node.keys.get(channel)) else {
            return Vec::new();
        };
        // SAFETY: the buffer holds `cap` keys; the shim writes at most that many.
        fill_vec(*len, |out, cap| unsafe {
            u::cu_bake_keys(self.ptr(), node.slot, *id, out, cap)
        })
    }
}

impl Drop for Bake<'_> {
    fn drop(&mut self) {
        // SAFETY: the bake came from cu_bake_stack and is freed once.
        unsafe { u::cu_bake_free(self.raw.as_ptr()) }
    }
}

/// Channel ids for `Bake::keys`.
pub(crate) const CHANNELS: [u32; 3] = [
    u::CU_CHANNEL_TRANSLATION,
    u::CU_CHANNEL_ROTATION,
    u::CU_CHANNEL_SCALE,
];
