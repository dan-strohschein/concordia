//! Settings validation and the per-device size choice (M4 4.2 Q2).

use concordia_cluster::{ClusterError, MeshletBuildSettings, MeshletSize};

#[test]
fn settings_are_validated() {
    use MeshletSize::*;
    assert!(MeshletBuildSettings::new(S128, 128, 0.25).is_ok());
    assert!(MeshletBuildSettings::new(S64, 64, 0.0).is_ok());
    assert!(MeshletBuildSettings::new(S64, 128, 1.0).is_ok());
    for (v, w) in [
        (63, 0.25),
        (129, 0.25),
        (128, -0.01),
        (128, 1.01),
        (128, f32::NAN),
    ] {
        assert!(
            matches!(
                MeshletBuildSettings::new(S128, v, w),
                Err(ClusterError::Settings(_))
            ),
            "{v} {w}"
        );
    }
}

#[test]
fn the_default_is_128_triangles_and_vertices() {
    let s = MeshletBuildSettings::default();
    assert_eq!(
        (s.size(), s.size().triangles(), s.max_vertices()),
        (MeshletSize::S128, 128, 128)
    );
    assert_eq!(s.cone_weight(), 0.25);
    let small = MeshletBuildSettings::for_size(MeshletSize::S64);
    assert_eq!((small.size().triangles(), small.max_vertices()), (64, 64));
}

#[test]
fn size_for_limits_picks_128_then_64() {
    use MeshletSize::*;
    assert_eq!(
        MeshletSize::for_limits(true, 256, 512).unwrap(),
        S128,
        "Metal"
    );
    assert_eq!(
        MeshletSize::for_limits(true, 256, 256).unwrap(),
        S128,
        "Vulkan minimum"
    );
    assert_eq!(MeshletSize::for_limits(true, 128, 128).unwrap(), S128);
    assert_eq!(MeshletSize::for_limits(true, 127, 512).unwrap(), S64);
    assert_eq!(MeshletSize::for_limits(true, 256, 100).unwrap(), S64);
    assert_eq!(
        MeshletSize::for_limits(false, 0, 0).unwrap(),
        S128,
        "compute fallback"
    );
    assert_eq!(
        MeshletSize::for_limits(true, 63, 512).unwrap_err(),
        ClusterError::Limits {
            vertices: 63,
            primitives: 512
        }
    );
}
