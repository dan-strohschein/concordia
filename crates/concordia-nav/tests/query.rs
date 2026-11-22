//! Navmesh queries: paths, costs, snapping, partial paths and surface raycasts.
//!
//! The rules in `build.rs`' header apply: a floor of half 10.1 is walkable to ±9.75, blocked cells
//! grow by 0.25 m except at their corners, and the surface of a floor at y = 0 is at y = 0.2
//! (`SURFACE`). The floor is one tile (128 cells): Detour's A* runs over portal midpoints, so a
//! corridor crossing tile seams can bend at a seam, and these tests pin path shapes.

mod common;

use common::{Level, SURFACE, close};
use concordia_core::math::Vec3;
use concordia_nav::{NavAreaKind, NavError};

fn floor() -> Level {
    let mut level = Level::new();
    level.slab(0.0, 0.0, 10.1, 10.1, 0.0);
    let mut settings = common::volume_settings(Vec3::new(12.0, 3.0, 12.0));
    settings.tile_size = 128;
    level.volume(Vec3::ZERO, settings);
    level
}

fn on(x: f32, z: f32) -> Vec3 {
    Vec3::new(x, SURFACE, z)
}

#[test]
fn path_query_returns_straight_path_on_open_floor() {
    let mesh = floor().build();
    let path = mesh
        .path(Vec3::new(-5.0, 0.0, -5.0), Vec3::new(6.0, 0.0, 3.0))
        .unwrap();
    assert!(!path.partial);
    assert_eq!(path.waypoints.len(), 2, "{path:?}");
    assert!(close(path.waypoints[0], on(-5.0, -5.0), 1e-3), "{path:?}");
    assert!(close(path.waypoints[1], on(6.0, 3.0), 1e-3), "{path:?}");
    // √(11² + 8²) = √185.
    assert!(
        (path.length - 185f32.sqrt()).abs() < 1e-3,
        "{}",
        path.length
    );
}

#[test]
fn path_avoids_blocked_cells() {
    // A wall of blocked cells at x ∈ [−1, 1), z ∈ [−10, 6): the gap is z ∈ [6, 10).
    let mut level = floor();
    let mut rows = vec![".".repeat(20); 20];
    for row in &mut rows[..16] {
        row.replace_range(9..11, "##");
    }
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    level.grid(-10.0, -10.0, 1.0, &rows);
    let mesh = level.build();
    let path = mesh
        .path(Vec3::new(-5.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0))
        .unwrap();
    assert!(!path.partial);
    // The eroded wall end is x ∈ [−1.25, 1.25] up to z = 6, then x ∈ [−1, 1] up to 6.25 (the
    // corner cells stay). The taut path touches both steps on each side:
    // 2 × √(3.75² + 6²) + 2 × √(0.25² + 0.25²) + 2.
    let expected = [
        on(-5.0, 0.0),
        on(-1.25, 6.0),
        on(-1.0, 6.25),
        on(1.0, 6.25),
        on(1.25, 6.0),
        on(5.0, 0.0),
    ];
    assert_eq!(path.waypoints.len(), expected.len(), "{path:?}");
    for (got, want) in path.waypoints.iter().zip(expected) {
        assert!(close(*got, want, 1e-3), "{got} vs {want}");
    }
    let length = 2.0 * (3.75f32 * 3.75 + 36.0).sqrt() + 2.0 * 0.125f32.sqrt() + 2.0;
    assert!(
        (path.length - length).abs() < 1e-3,
        "{} vs {length}",
        path.length
    );
    for pair in path.waypoints.windows(2) {
        for i in 0..=100 {
            let p = pair[0].lerp(pair[1], i as f32 / 100.0);
            assert!(
                !(p.x > -1.0 && p.x < 1.0 && p.z < 6.0),
                "{p} is in the wall"
            );
        }
    }
}

#[test]
fn area_costs_steer_paths() {
    // A costly strip x ∈ [−1, 1], z ∈ [−10, 3]; the cheap way round is through z ∈ [3, 9.75].
    let strip = |cost: f32| {
        let mut level = floor();
        level.area(
            Vec3::new(0.0, 0.0, -3.5),
            Vec3::new(1.0, 2.0, 6.5),
            NavAreaKind::Cost { cost },
        );
        level.build()
    };
    let (from, to) = (Vec3::new(-8.0, 0.0, 0.0), Vec3::new(8.0, 0.0, 0.0));
    let cheap = strip(1.0).path(from, to).unwrap();
    assert_eq!(
        cheap.waypoints.len(),
        2,
        "cost 1 is plain ground: {cheap:?}"
    );
    assert!((cheap.length - 16.0).abs() < 1e-3);
    let dear = strip(10.0).path(from, to).unwrap();
    assert!(
        dear.waypoints.iter().any(|p| p.z >= 3.0 - 1e-3),
        "goes round the strip: {dear:?}"
    );
    // Round the strip's corners (±1, 3): 2 × √(7² + 3²) + 2.
    let round = 2.0 * (49f32 + 9.0).sqrt() + 2.0;
    assert!(
        (dear.length - round).abs() < 1e-3,
        "{} vs {round}",
        dear.length
    );
    let polys = strip(10.0);
    assert!(
        polys.polygons().iter().any(|p| p.area == 2),
        "the first cost area id is 2"
    );
}

#[test]
fn nearest_point_snaps_click_to_navmesh() {
    let mut level = floor();
    // One blocked cell [2, 3] × [2, 3], eroded to [1.75, 3.25]² less its corner cells.
    let mut rows = vec![".".repeat(20); 20];
    rows[12].replace_range(12..13, "#");
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    level.grid(-10.0, -10.0, 1.0, &rows);
    let mesh = level.build();
    let open = mesh.nearest_point(Vec3::new(3.3, 0.0, -2.7)).unwrap();
    assert!(close(open.position, on(3.3, -2.7), 1e-3), "{open:?}");
    let raised = mesh.nearest_point(Vec3::new(3.3, 1.5, -2.7)).unwrap();
    assert!(
        close(raised.position, on(3.3, -2.7), 1e-3),
        "a click 1.5 m up drops to the floor: {raised:?}"
    );
    // Inside the blocked cell at (2.4, 2.6), the nearest walkable point is the inner corner of the
    // kept corner cell [1.75, 2] × [3, 3.25]: (2, 3), 0.566 away (the z = 3.25 and x = 1.75 edges
    // are 0.65 away, the corner (3, 3) 0.72).
    let blocked = mesh.nearest_point(Vec3::new(2.4, 0.0, 2.6)).unwrap();
    assert!(close(blocked.position, on(2.0, 3.0), 1e-3), "{blocked:?}");
    assert!(
        (blocked.position - Vec3::new(2.4, 0.0, 2.6)).length() < 1.0,
        "within one grid cell"
    );
    assert_eq!(
        mesh.nearest_point(Vec3::new(40.0, 0.0, 0.0)),
        None,
        "beyond the search reach"
    );
    assert_eq!(
        mesh.island_at(Vec3::new(2.4, 0.0, 2.6)),
        None,
        "not on the navmesh"
    );
    assert_eq!(mesh.island_at(Vec3::new(3.3, 0.0, -2.7)), Some(0));
}

#[test]
fn unreachable_target_returns_partial_path_to_closest_point() {
    // Two floors with a 1.8 m gap: x ∈ [−10.1, −0.9] and [0.9, 10.1]; the left one is walkable to
    // x = −1.25.
    let mut level = Level::new();
    level.slab(-5.5, 0.0, 4.6, 5.1, 0.0);
    level.slab(5.5, 0.0, 4.6, 5.1, 0.0);
    level.standard_volume(12.0);
    let mesh = level.build();
    assert_eq!(mesh.stats().islands, 2);
    let path = mesh
        .path(Vec3::new(-6.0, 0.0, 0.0), Vec3::new(6.0, 0.0, 0.0))
        .unwrap();
    assert!(path.partial, "{path:?}");
    let last = *path.waypoints.last().unwrap();
    assert!(
        close(last, on(-1.25, 0.0), 1e-3),
        "the closest reachable point: {last}"
    );
    assert!((path.length - 4.75).abs() < 1e-3, "{}", path.length);
    assert!(matches!(
        mesh.path(Vec3::new(40.0, 0.0, 0.0), Vec3::ZERO),
        Err(NavError::OffNavmesh(_))
    ));
}

#[test]
fn raycast_stops_at_the_navmesh_edge() {
    let mesh = floor().build();
    let ray = mesh
        .raycast(Vec3::new(0.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0))
        .unwrap();
    let hit = ray.hit.expect("hits the edge");
    assert!(close(hit, on(9.75, 0.0), 1e-3), "{hit}");
    assert!(close(ray.normal, Vec3::NEG_X, 1e-3), "{:?}", ray.normal);
    let free = mesh
        .raycast(Vec3::new(0.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 5.0))
        .unwrap();
    assert_eq!(free.hit, None);
    assert_eq!(free.normal, Vec3::ZERO);
}

#[test]
fn queries_from_many_threads_agree() {
    let mesh = std::sync::Arc::new(floor().build());
    let one = mesh
        .path(Vec3::new(-7.0, 0.0, 2.0), Vec3::new(7.5, 0.0, -3.0))
        .unwrap();
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let mesh = mesh.clone();
            std::thread::spawn(move || {
                (0..50)
                    .map(|_| {
                        mesh.path(Vec3::new(-7.0, 0.0, 2.0), Vec3::new(7.5, 0.0, -3.0))
                            .unwrap()
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    for h in handles {
        assert!(h.join().unwrap().iter().all(|p| *p == one));
    }
}
