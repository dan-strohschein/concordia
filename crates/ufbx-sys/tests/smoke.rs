//! ufbx-sys: binding drift, ABI sizes, loading, bounds checks and the fixtures' declared content.

use std::path::Path;
use std::ptr;

use ufbx_sys as u;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(root().join("tests/fixtures/fbx").join(name)).unwrap()
}

/// A loaded scene, freed on drop.
struct Scene(*mut u::cu_scene);

impl Scene {
    fn load(bytes: &[u8]) -> Result<Self, String> {
        let mut error = [0 as std::os::raw::c_char; 512];
        // SAFETY: the pointers and lengths describe live buffers.
        let scene =
            unsafe { u::cu_load(bytes.as_ptr(), bytes.len(), error.as_mut_ptr(), error.len()) };
        if scene.is_null() {
            // SAFETY: the shim NUL-terminates the message within the buffer.
            let message = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) };
            return Err(message.to_string_lossy().into_owned());
        }
        Ok(Self(scene))
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        // SAFETY: the scene came from cu_load and is freed once.
        unsafe { u::cu_free(self.0) }
    }
}

fn str_of(s: u::cu_str) -> String {
    assert!(!s.data.is_null() || s.len == 0);
    if s.len == 0 {
        return String::new();
    }
    // SAFETY: the shim returns (data, len) of a string owned by a live scene.
    let bytes = unsafe { std::slice::from_raw_parts(s.data.cast::<u8>(), s.len) };
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn settings(scene: &Scene) -> u::cu_settings {
    let mut s = u::cu_settings {
        original_unit_meters: 0.0,
        frames_per_second: 0.0,
        original_up_axis: -1,
    };
    // SAFETY: valid scene and out pointer.
    assert!(unsafe { u::cu_settings_get(scene.0, &mut s) });
    s
}

#[test]
fn bindings_match_the_shim_header() {
    let header = std::fs::read(root().join("crates/ufbx-sys/shim/concordia_ufbx.h")).unwrap();
    let expected = format!("// header blake3: {}", blake3::hash(&header).to_hex());
    let bindings = std::fs::read_to_string(root().join("crates/ufbx-sys/src/bindings.rs")).unwrap();
    assert!(
        bindings.lines().any(|l| l == expected),
        "the shim header changed: update src/bindings.rs by hand, then its hash line ({expected})"
    );
}

#[test]
fn struct_sizes_match_the_c_compiler() {
    let rust = [
        size_of::<u::cu_str>(),
        size_of::<u::cu_bytes>(),
        size_of::<u::cu_settings>(),
        size_of::<u::cu_node>(),
        size_of::<u::cu_mesh>(),
        size_of::<u::cu_corner>(),
        size_of::<u::cu_skin>(),
        size_of::<u::cu_cluster>(),
        size_of::<u::cu_weight>(),
        size_of::<u::cu_material>(),
        size_of::<u::cu_texture_file>(),
        size_of::<u::cu_stack>(),
        size_of::<u::cu_baked_node>(),
        size_of::<u::cu_key>(),
    ];
    for (which, size) in rust.iter().enumerate() {
        // SAFETY: a pure function of its argument.
        let c = unsafe { u::cu_struct_size(which as u32) };
        assert_eq!(c, *size, "struct #{which}");
    }
    // SAFETY: as above.
    assert_eq!(unsafe { u::cu_struct_size(rust.len() as u32) }, 0);
}

#[test]
fn fixtures_report_the_units_and_axes_they_declare() {
    let cm = settings(&Scene::load(&fixture("units_cm_zup.fbx")).unwrap());
    assert_eq!(
        (cm.original_unit_meters, cm.original_up_axis),
        (0.01, 4),
        "cm, +Z up"
    );
    let m = settings(&Scene::load(&fixture("static_quad.fbx")).unwrap());
    assert_eq!(
        (m.original_unit_meters, m.original_up_axis),
        (1.0, 2),
        "m, +Y up"
    );
    assert_eq!(m.frames_per_second, 24.0);
}

#[test]
fn garbage_empty_and_truncated_files_fail_with_a_message() {
    let quad = fixture("static_quad.fbx");
    for (what, bytes) in [
        ("garbage", b"this is not an fbx file at all".to_vec()),
        ("empty", Vec::new()),
        ("truncated", quad[..quad.len() / 3].to_vec()),
    ] {
        let error = Scene::load(&bytes)
            .err()
            .unwrap_or_else(|| panic!("{what} loaded"));
        assert!(!error.is_empty(), "{what}: empty message");
    }
    // SAFETY: a null buffer with length 0 and a null error buffer must be refused, not read.
    assert!(unsafe { u::cu_load(ptr::null(), 0, ptr::null_mut(), 0) }.is_null());
}

#[test]
fn null_and_out_of_range_arguments_are_refused() {
    let scene = Scene::load(&fixture("static_quad.fbx")).unwrap();
    // SAFETY: every call passes a null or valid scene and valid or null out pointers.
    unsafe {
        let mut node = std::mem::zeroed::<u::cu_node>();
        assert!(!u::cu_node_get(ptr::null(), 0, &mut node));
        assert!(!u::cu_node_get(scene.0, 1_000, &mut node));
        assert!(!u::cu_node_get(scene.0, 0, ptr::null_mut()));
        assert_eq!(u::cu_node_count(ptr::null()), 0);
        assert_eq!(u::cu_node_material(scene.0, 1_000, 0), u::CU_NONE);
        let mut corners = [u::cu_corner::default(); 4];
        assert_eq!(
            u::cu_mesh_part_corners(scene.0, 1_000, 0, corners.as_mut_ptr(), 4),
            0
        );
        assert_eq!(
            u::cu_mesh_part_corners(scene.0, 0, 1_000, corners.as_mut_ptr(), 4),
            0
        );
        assert_eq!(
            u::cu_mesh_part_corners(scene.0, 0, 0, ptr::null_mut(), 4),
            0
        );
        assert_eq!(
            u::cu_mesh_part_corners(scene.0, 0, 0, corners.as_mut_ptr(), 0),
            0
        );
        assert!(u::cu_bake_stack(scene.0, 1_000, 24.0, ptr::null_mut(), 0).is_null());
        assert_eq!(u::cu_bake_keys(ptr::null(), 0, 0, ptr::null_mut(), 0), 0);
        u::cu_free(ptr::null_mut());
        u::cu_bake_free(ptr::null_mut());
    }
}

#[test]
fn quad_corners_come_triangulated_with_attributes() {
    let scene = Scene::load(&fixture("static_quad.fbx")).unwrap();
    // SAFETY: valid scene, out pointers and capacities.
    unsafe {
        assert_eq!(u::cu_mesh_count(scene.0), 2);
        let mut mesh = std::mem::zeroed::<u::cu_mesh>();
        assert!(u::cu_mesh_get(scene.0, 0, &mut mesh));
        assert!(mesh.has_normals && mesh.has_uvs);
        assert_eq!(
            (mesh.num_parts, mesh.num_vertices, mesh.skin),
            (1, 4, u::CU_NONE)
        );
        assert_eq!(u::cu_mesh_part_triangles(scene.0, 0, 0), 2);
        let mut corners = [u::cu_corner::default(); 8];
        assert_eq!(
            u::cu_mesh_part_corners(scene.0, 0, 0, corners.as_mut_ptr(), 8),
            6
        );
        assert_eq!(
            u::cu_mesh_part_corners(scene.0, 0, 0, corners.as_mut_ptr(), 2),
            2,
            "cap"
        );
        assert!(corners.iter().take(6).all(|c| c.normal == [0.0, 1.0, 0.0]));
    }
}

#[test]
fn skinned_fixture_vertex_zero_has_seven_influences_largest_first() {
    let scene = Scene::load(&fixture("skinned.fbx")).unwrap();
    // SAFETY: valid scene, out pointers and capacities.
    unsafe {
        let mut mesh = std::mem::zeroed::<u::cu_mesh>();
        assert!(u::cu_mesh_get(scene.0, 0, &mut mesh));
        assert!(mesh.skin >= 0);
        let mut skin = u::cu_skin::default();
        assert!(u::cu_skin_get(scene.0, mesh.skin as usize, &mut skin));
        assert_eq!(skin.num_clusters, 7);
        let mut weights = [u::cu_weight::default(); 8];
        let n = u::cu_skin_weights(scene.0, mesh.skin as usize, 0, weights.as_mut_ptr(), 8);
        assert_eq!(n, 7, "the fixture needs more than four influences");
        assert!(weights[..n].windows(2).all(|w| w[0].weight >= w[1].weight));
        let mut cluster = u::cu_cluster::default();
        assert!(u::cu_skin_cluster(
            scene.0,
            mesh.skin as usize,
            0,
            &mut cluster
        ));
        let mut bone = std::mem::zeroed::<u::cu_node>();
        assert!(u::cu_node_get(scene.0, cluster.bone as usize, &mut bone));
        assert_eq!(str_of(bone.name), "mixamorig:Hips");
    }
}

#[test]
fn stacks_bake_at_the_requested_rate_from_zero() {
    let scene = Scene::load(&fixture("anim_stacks.fbx")).unwrap();
    // SAFETY: valid scene, out pointers and capacities; the bake is freed once.
    unsafe {
        assert_eq!(u::cu_stack_count(scene.0), 2);
        let mut stack = std::mem::zeroed::<u::cu_stack>();
        assert!(u::cu_stack_get(scene.0, 1, &mut stack));
        assert_eq!(str_of(stack.name), "Armature|Walk");
        let bake = u::cu_bake_stack(scene.0, 1, 24.0, ptr::null_mut(), 0);
        assert!(!bake.is_null());
        assert_eq!(u::cu_bake_duration(bake), 1.0);
        assert!(u::cu_bake_node_count(bake) > 0);
        let mut node = u::cu_baked_node::default();
        assert!(u::cu_bake_node_get(bake, 0, &mut node));
        let mut keys = vec![u::cu_key::default(); node.num_keys[1] as usize];
        let n = u::cu_bake_keys(
            bake,
            0,
            u::CU_CHANNEL_ROTATION,
            keys.as_mut_ptr(),
            keys.len(),
        );
        assert_eq!(n, 25);
        assert_eq!((keys[0].time, keys[24].time), (0.0, 1.0));
        assert!((keys[1].time - 1.0 / 24.0).abs() < 1e-6);
        u::cu_bake_free(bake);
    }
}
