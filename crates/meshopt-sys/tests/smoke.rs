//! meshopt-sys: binding drift, ABI sizes, the vendored version and the checked wrapper's refusals.

use std::path::Path;

use meshopt_sys::{MeshoptError, ffi, safe};
use proptest::prelude::*;

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Two triangles forming a unit quad in the XY plane, facing +Z.
fn quad() -> (Vec<u32>, Vec<[f32; 3]>) {
    let positions = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    (vec![0, 1, 2, 0, 2, 3], positions)
}

#[test]
fn header_hash_matches_bindings() {
    let header =
        std::fs::read(root().join("third_party/meshoptimizer/src/meshoptimizer.h")).unwrap();
    let expected = format!("// header blake3: {}", blake3::hash(&header).to_hex());
    let bindings =
        std::fs::read_to_string(root().join("crates/meshopt-sys/src/bindings.rs")).unwrap();
    assert!(
        bindings.lines().any(|l| l == expected),
        "meshoptimizer.h changed: update src/bindings.rs by hand, then its hash line ({expected})"
    );
}

#[test]
fn struct_sizes_match_the_c_compiler() {
    let rust = [
        size_of::<ffi::meshopt_Meshlet>(),
        size_of::<ffi::meshopt_Bounds>(),
    ];
    for (which, size) in rust.iter().enumerate() {
        // SAFETY: the shim only reads its argument.
        let c = unsafe { ffi::cm_struct_size(which as u32) };
        assert_eq!(c, *size, "struct #{which}");
    }
    // SAFETY: as above.
    assert_eq!(unsafe { ffi::cm_struct_size(rust.len() as u32) }, 0);
}

#[test]
fn vendored_version_is_1_3() {
    // SAFETY: the shim returns a constant.
    assert_eq!(unsafe { ffi::cm_version() }, 1030);
    assert_eq!(meshopt_sys::MESHOPTIMIZER_VERSION, "1.3");
}

#[test]
fn a_quad_builds_one_meshlet() {
    let (indices, positions) = quad();
    let mut built = safe::build_meshlets(&indices, &positions, 64, 64, 0.25).unwrap();
    assert_eq!(built.meshlets.len(), 1);
    let m = built.meshlets[0];
    assert_eq!((m.vertex_count, m.triangle_count), (4, 2));
    assert_eq!(built.vertices.len(), 4);
    assert_eq!(built.triangles.len(), 6);

    safe::optimize_meshlet(&mut built.vertices, &mut built.triangles).unwrap();
    let bounds = safe::meshlet_bounds(&built.vertices, &built.triangles, &positions).unwrap();
    for p in &positions {
        let d = (0..3)
            .map(|i| (p[i] - bounds.center[i]).powi(2))
            .sum::<f32>()
            .sqrt();
        assert!(
            d <= bounds.radius * 1.0001 + 1e-6,
            "{p:?} outside the sphere"
        );
    }
    assert!(bounds.cone_axis[2] > 0.99, "a flat quad's cone faces +Z");
}

#[test]
fn limits_beyond_meshoptimizer_are_refused() {
    let (indices, positions) = quad();
    let build = |i: &[u32], p: &[[f32; 3]], v, t, w| safe::build_meshlets(i, p, v, t, w);
    assert_eq!(
        build(&indices, &positions, 2, 64, 0.0).unwrap_err(),
        MeshoptError::MaxVertices(2)
    );
    assert_eq!(
        build(&indices, &positions, 257, 64, 0.0).unwrap_err(),
        MeshoptError::MaxVertices(257)
    );
    assert_eq!(
        build(&indices, &positions, 64, 0, 0.0).unwrap_err(),
        MeshoptError::MaxTriangles(0)
    );
    assert_eq!(
        build(&indices, &positions, 64, 513, 0.0).unwrap_err(),
        MeshoptError::MaxTriangles(513)
    );
    assert!(matches!(
        build(&indices, &positions, 64, 64, -0.1).unwrap_err(),
        MeshoptError::ConeWeight(_)
    ));
    assert!(matches!(
        build(&indices, &positions, 64, 64, f32::NAN).unwrap_err(),
        MeshoptError::ConeWeight(_)
    ));
    assert_eq!(
        build(&indices[..4], &positions, 64, 64, 0.0).unwrap_err(),
        MeshoptError::IndexCount(4)
    );
    assert_eq!(
        build(&[0, 1, 9], &positions, 64, 64, 0.0).unwrap_err(),
        MeshoptError::IndexRange {
            index: 9,
            vertices: 4
        }
    );
    let mut bad = positions.clone();
    bad[2][1] = f32::INFINITY;
    assert_eq!(
        build(&indices, &bad, 64, 64, 0.0).unwrap_err(),
        MeshoptError::NonFinitePosition(2)
    );

    let mut vertices = vec![0u32; 257];
    let mut triangles = vec![0u8, 1, 2];
    assert!(matches!(
        safe::optimize_meshlet(&mut vertices, &mut triangles).unwrap_err(),
        MeshoptError::Meshlet(_)
    ));
    let mut vertices = vec![0u32, 1, 2];
    let mut triangles = vec![0u8, 1, 3];
    assert!(matches!(
        safe::optimize_meshlet(&mut vertices, &mut triangles).unwrap_err(),
        MeshoptError::Meshlet(_)
    ));
    let mut triangles = vec![0u8, 1];
    assert!(matches!(
        safe::optimize_meshlet(&mut vertices, &mut triangles).unwrap_err(),
        MeshoptError::Meshlet(_)
    ));
    assert!(matches!(
        safe::meshlet_bounds(&[0, 1, 7], &[0, 1, 2], &positions).unwrap_err(),
        MeshoptError::IndexRange { index: 7, .. }
    ));
}

proptest! {
    /// Random soups: the wrapper's output is trimmed, in range and within the limits.
    #[test]
    fn built_meshlets_stay_within_limits_and_buffers(
        positions in prop::collection::vec(prop::array::uniform3(-10.0f32..10.0), 3..60),
        picks in prop::collection::vec(any::<prop::sample::Index>(), 3..300),
        max_vertices in 3usize..=256,
        max_triangles in 1usize..=512,
    ) {
        let count = picks.len() / 3 * 3;
        let indices: Vec<u32> =
            picks[..count].iter().map(|i| i.index(positions.len()) as u32).collect();
        let built = safe::build_meshlets(&indices, &positions, max_vertices, max_triangles, 0.5).unwrap();
        let mut triangles = 0;
        for m in &built.meshlets {
            prop_assert!(m.vertex_count as usize <= max_vertices);
            prop_assert!(m.triangle_count as usize <= max_triangles);
            let v_end = (m.vertex_offset + m.vertex_count) as usize;
            let t_end = (m.triangle_offset + m.triangle_count * 3) as usize;
            prop_assert!(v_end <= built.vertices.len() && t_end <= built.triangles.len());
            triangles += m.triangle_count as usize;
        }
        prop_assert!(triangles <= count / 3);
        prop_assert!(built.vertices.iter().all(|&v| (v as usize) < positions.len()));
    }
}
