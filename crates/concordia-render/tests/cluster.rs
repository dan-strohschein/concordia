//! Clustered lighting: grid, slices and the CPU reference assignment (M1 phase 1.6).

use concordia_core::math::Vec3;
use concordia_render::Projection;
use concordia_render::cluster::{ClusterGrid, ClusterSettings, LightBounds};

fn persp() -> ClusterGrid {
    ClusterGrid::new(
        ClusterSettings::default(),
        &Projection::Perspective {
            fov_y: 1.0,
            near: 0.1,
            far: 100.0,
        },
        16.0 / 9.0,
    )
    .unwrap()
}

fn ortho() -> ClusterGrid {
    ClusterGrid::new(
        ClusterSettings::default(),
        &Projection::Orthographic {
            height: 20.0,
            near: 1.0,
            far: 61.0,
        },
        16.0 / 9.0,
    )
    .unwrap()
}

#[test]
fn ortho_camera_uses_linear_depth_slices_and_perspective_exponential() {
    let o = ortho().slice_boundaries();
    assert!(
        o.windows(2).all(|w| ((w[1] - w[0]) - 2.5).abs() < 1e-4),
        "{o:?}"
    );
    let p = persp().slice_boundaries();
    let ratios: Vec<f32> = p.windows(2).map(|w| w[1] / w[0]).collect();
    assert!(
        ratios.iter().all(|r| (r - ratios[0]).abs() < 1e-3),
        "{ratios:?}"
    );
    insta::assert_debug_snapshot!((
        p.iter()
            .map(|z| (z * 1e4).round() / 1e4)
            .collect::<Vec<_>>(),
        o
    ));
}

#[test]
fn cluster_aabbs_tile_the_view_volume() {
    let grid = ortho();
    let (min, max) = grid.aabb(0);
    assert!(
        (min.x + 20.0 * 16.0 / 9.0 / 2.0).abs() < 1e-4 && (max.y - 10.0).abs() < 1e-4,
        "cluster 0 is the top-left tile: {min} {max}"
    );
    assert!(
        (max.z + 1.0).abs() < 1e-5 && (min.z + 3.5).abs() < 1e-5,
        "first slice spans view z -1..-3.5: {min} {max}"
    );
    assert_eq!(grid.len(), 16 * 9 * 24);
}

#[test]
fn light_outside_frustum_is_in_no_cluster() {
    let behind = LightBounds {
        center: Vec3::new(0.0, 0.0, 5.0),
        radius: 1.0,
    };
    let beside = LightBounds {
        center: Vec3::new(500.0, 0.0, -10.0),
        radius: 2.0,
    };
    for grid in [persp(), ortho()] {
        let lists = grid.assign(&[behind, beside]);
        assert!(lists.lists.iter().all(Vec::is_empty));
        assert_eq!(lists.overflow, 0);
    }
}

#[test]
fn a_light_reaches_exactly_the_clusters_its_sphere_touches() {
    let grid = ortho();
    let light = LightBounds {
        center: Vec3::new(0.0, 0.0, -10.0),
        radius: 1.0,
    };
    let lists = grid.assign(&[light]);
    let touched = lists.lists.iter().filter(|l| l.contains(&0)).count();
    assert!(
        (2..=16).contains(&touched),
        "a small sphere touches a handful of clusters: {touched}"
    );
}

#[test]
fn cluster_overflow_is_counted() {
    let settings = ClusterSettings {
        max_per_cluster: 4,
        ..ClusterSettings::default()
    };
    let grid = ClusterGrid::new(
        settings,
        &Projection::Orthographic {
            height: 20.0,
            near: 1.0,
            far: 61.0,
        },
        1.0,
    )
    .unwrap();
    let lights = vec![
        LightBounds {
            center: Vec3::new(0.0, 0.0, -10.0),
            radius: 0.1
        };
        10
    ];
    let lists = grid.assign(&lights);
    assert!(lists.lists.iter().all(|l| l.len() <= 4));
    assert!(lists.overflow > 0);
}

#[test]
fn invalid_settings_are_errors() {
    let bad = ClusterSettings {
        dims: [0, 9, 24],
        ..ClusterSettings::default()
    };
    assert!(ClusterGrid::new(bad, &Projection::default(), 1.0).is_err());
    let too_many = ClusterSettings {
        max_per_cluster: 0,
        ..ClusterSettings::default()
    };
    assert!(ClusterGrid::new(too_many, &Projection::default(), 1.0).is_err());
}
