//! Navmesh builds: sources, marks, reachability, tiling, determinism, validation and the cache.
//!
//! Fixture rules (0.25 m cells on integer-aligned boundaries, a 0.25 m radius):
//! - Floor edges sit 0.1 m inside a cell (a floor of half 10 is 10.1), because Recast gives a
//!   face exactly on a cell boundary to one side only. The partly covered outer cell is a ledge
//!   (`rcFilterLedgeSpans`: nothing beside it), and erosion removes one more cell: the floor is
//!   walkable to ±9.75.
//! - Marks cover the cells they overlap with exclusive upper bounds, and blocked marks apply
//!   before erosion. Erosion is Recast's distance transform (an orthogonal step costs 2, a
//!   diagonal one 3) removing distances below 2 × one cell: the cells beside a blocked cell go,
//!   but the cell diagonal to a blocked corner stays. A blocked square [−2, 2) grows to
//!   [−2.25, 2.25] less its four corner cells.

mod common;

use std::num::NonZeroUsize;

use common::{Level, SURFACE, close, ground_area, volume_settings};
use concordia_core::ddc::Ddc;
use concordia_core::math::{Quat, Vec3};
use concordia_nav::{
    NavArea, NavAreaGrid, NavAreaKind, NavCache, NavError, NavInput, NavMeshData, NavMeshVolume,
    build_tiles,
};
use concordia_physics::{BodyKind, Collider, ColliderShape, RigidBody};
use concordia_scene::builtin::Transform;
use concordia_terra::World;

/// A floor walkable to ±(half − 0.25) (see the header).
fn floor(half: f32) -> Level {
    let mut level = Level::new();
    level.slab(0.0, 0.0, half + 0.1, half + 0.1, 0.0);
    level.standard_volume(half + 2.0);
    level
}

/// A level with every kind of source and mark.
fn busy_level() -> Level {
    let mut level = floor(20.0);
    for i in 0..4 {
        let x = -12.0 + 8.0 * i as f32;
        level.block(
            Vec3::new(x, 0.0, -10.0),
            Vec3::new(0.8, 1.5, 0.8),
            Quat::IDENTITY,
        );
    }
    level.block(
        Vec3::new(6.0, 0.0, 6.0),
        Vec3::new(2.0, 1.0, 0.5),
        Quat::from_rotation_y(0.5),
    );
    let pillar = Collider::new(ColliderShape::Cylinder {
        half_height: 1.0,
        radius: 0.7,
    });
    level.body(
        Vec3::new(-6.0, 1.0, 8.0),
        Quat::IDENTITY,
        BodyKind::Static,
        pillar,
    );
    let ramp = Collider::new(ColliderShape::Box {
        half_extents: Vec3::new(3.0, 0.2, 1.5),
    });
    level.body(
        Vec3::new(10.0, 0.5, -2.0),
        Quat::from_rotation_z(0.3),
        BodyKind::Static,
        ramp,
    );
    level.area(
        Vec3::new(0.0, 0.0, 2.0),
        Vec3::new(3.0, 2.0, 1.0),
        NavAreaKind::Cost { cost: 4.0 },
    );
    level.area(
        Vec3::new(-10.0, 0.0, 2.0),
        Vec3::new(1.0, 2.0, 1.0),
        NavAreaKind::Blocked,
    );
    level.grid(12.0, 10.0, 1.0, &["..##..", ".####.", "..##.."]);
    level
}

/// The same entities (same ids and components) inserted in reverse id order.
fn reversed(level: &Level) -> World {
    let mut world = World::new();
    for id in level.world.entity_ids_sorted().into_iter().rev() {
        world.spawn_with_id(id).unwrap();
        let w = &level.world;
        if let Some(c) = w.get::<Transform>(id) {
            world.insert(id, *c).unwrap();
        }
        if let Some(c) = w.get::<RigidBody>(id) {
            world.insert(id, c.clone()).unwrap();
        }
        if let Some(c) = w.get::<Collider>(id) {
            world.insert(id, c.clone()).unwrap();
        }
        if let Some(c) = w.get::<NavMeshVolume>(id) {
            world.insert(id, c.clone()).unwrap();
        }
        if let Some(c) = w.get::<NavArea>(id) {
            world.insert(id, c.clone()).unwrap();
        }
        if let Some(c) = w.get::<NavAreaGrid>(id) {
            world.insert(id, c.clone()).unwrap();
        }
    }
    world
}

fn threads(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

#[test]
fn navmesh_build_is_deterministic_byte_identical() {
    let level = busy_level();
    let input = level.input();
    assert!(input.settings.tile_width() == 16.0, "64 cells of 0.25 m");
    let one = build_tiles(&input, threads(1)).unwrap().to_bytes();
    let eight = build_tiles(&input, threads(8)).unwrap().to_bytes();
    let again = build_tiles(&level.input(), threads(3)).unwrap().to_bytes();
    let volume = level.volume.unwrap();
    let flipped = build_tiles(
        &NavInput::gather(&reversed(&level), volume).unwrap(),
        threads(8),
    )
    .unwrap()
    .to_bytes();
    assert!(one.len() > 1000, "{} bytes", one.len());
    let hash = |b: &[u8]| blake3::hash(b).to_hex().to_string();
    assert_eq!(hash(&one), hash(&eight), "1 and 8 threads");
    assert_eq!(hash(&one), hash(&again), "a second build");
    assert_eq!(
        hash(&one),
        hash(&flipped),
        "entities inserted in reverse order"
    );
    assert_eq!(
        input.key(),
        NavInput::gather(&reversed(&level), volume).unwrap().key()
    );
    // A 44 m volume is ceil(44 / 16) = 3 tiles a side.
    let data = NavMeshData::from_bytes(&one).unwrap();
    assert_eq!(data.grid, [3, 3]);
    assert_eq!(data.tiles.len(), 9);
}

#[test]
fn static_colliders_are_the_navmesh_source() {
    let mut level = floor(10.0);
    let solid = |half: Vec3| Collider::new(ColliderShape::Box { half_extents: half });
    let crate_half = Vec3::new(0.5, 0.5, 0.5);
    level.body(
        Vec3::new(-5.0, 0.5, -5.0),
        Quat::IDENTITY,
        BodyKind::Dynamic,
        solid(crate_half),
    );
    level.body(
        Vec3::new(-5.0, 0.5, 5.0),
        Quat::IDENTITY,
        BodyKind::Kinematic,
        solid(crate_half),
    );
    let mut sensor = solid(Vec3::new(1.0, 1.0, 1.0));
    sensor.is_sensor = true;
    level.body(
        Vec3::new(5.0, 1.0, -5.0),
        Quat::IDENTITY,
        BodyKind::Static,
        sensor,
    );
    // Solid static obstacles: a box, a box turned 45° and a cylinder.
    level.block(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 1.0),
        Quat::IDENTITY,
    );
    level.block(
        Vec3::new(5.0, 0.0, 5.0),
        Vec3::new(1.0, 1.0, 1.0),
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
    );
    // A 6 m long box turned 90°: its footprint runs along z, not x.
    level.block(
        Vec3::new(0.0, 0.0, -6.0),
        Vec3::new(3.0, 1.0, 0.5),
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
    );
    let pillar = Collider::new(ColliderShape::Cylinder {
        half_height: 1.0,
        radius: 1.0,
    });
    level.body(
        Vec3::new(-5.0, 1.0, 0.0),
        Quat::IDENTITY,
        BodyKind::Static,
        pillar,
    );
    // A static box offset from its entity by its collider's offset.
    let mut offset = solid(Vec3::new(0.5, 1.0, 0.5));
    offset.offset = Vec3::new(0.0, 0.0, 3.0);
    level.body(
        Vec3::new(5.0, 1.0, 0.0),
        Quat::IDENTITY,
        BodyKind::Static,
        offset,
    );
    let mesh = level.build();
    let on = |x: f32, z: f32| {
        let p = mesh
            .nearest_point(Vec3::new(x, 0.0, z))
            .expect("near the navmesh")
            .position;
        close(p, Vec3::new(x, SURFACE, z), 1e-3)
    };
    for (x, z) in [(-5.0, -5.0), (-5.0, 5.0), (5.0, -5.0)] {
        assert!(
            on(x, z),
            "dynamic, kinematic and sensor bodies don't carve ({x}, {z})"
        );
    }
    // Inside each obstacle's footprint, 0.5 m from its centre, the ground isn't walkable.
    for (x, z) in [
        (0.0, 0.0),
        (0.5, 0.0),
        (5.0, 5.0),
        (5.0, 5.9),
        (-5.0, 0.0),
        (-5.0, 0.8),
        (5.0, 3.0),
    ] {
        assert!(!on(x, z), "a static obstacle carves ({x}, {z})");
    }
    // The 45° box's corners reach √2 m along the axes, but (5 + 1.2, 5 + 1.2) is outside it.
    assert!(
        on(6.6, 6.6),
        "the turned box carves its own footprint, not its bounds"
    );
    assert!(!on(0.0, -4.0), "the turned box covers z ∈ [−9, −3]");
    assert!(on(2.0, -6.0), "and only x ∈ [−0.5, 0.5]");
    assert!(
        on(5.0, 0.0),
        "the offset box sits at z = 3, not at its entity"
    );
    assert_eq!(
        mesh.stats().islands,
        1,
        "obstacle tops 2 m wide are too small for a region"
    );
}

#[test]
fn blocked_area_marks_carve_unwalkable_cells() {
    let mut level = floor(10.0);
    // A 20 × 20 grid of 1 m cells over the floor; a 4 × 4 block of cells at x, z ∈ [−2, 2).
    let mut rows = vec![".".repeat(20); 20];
    for row in &mut rows[8..12] {
        row.replace_range(8..12, "####");
    }
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    level.grid(-10.0, -10.0, 1.0, &rows);
    let mesh = level.build();
    // 19.5² of floor minus the blocked square grown by one cell (4.5²) less its corner cells.
    let area = ground_area(&mesh);
    let expected = 19.5 * 19.5 - (4.5 * 4.5 - 4.0 * 0.25 * 0.25);
    assert!(
        (area - expected).abs() < 1e-3,
        "area {area}, expected {expected}"
    );
    let snapped = mesh
        .nearest_point(Vec3::new(0.3, 0.0, 0.1))
        .expect("near")
        .position;
    assert!(
        snapped.x.abs() >= 2.25 - 1e-3 || snapped.z.abs() >= 2.25 - 1e-3,
        "{snapped}"
    );
    for poly in mesh.polygons() {
        let n = poly.vertices.len() as f32;
        let c = poly.vertices.iter().copied().sum::<Vec3>() / n;
        assert!(
            !(c.x.abs() < 2.0 && c.z.abs() < 2.0),
            "a polygon inside the blocked cells: {poly:?}"
        );
    }
    assert_eq!(mesh.stats().islands, 1);
}

#[test]
fn grid_walkable_marks_reopen_a_blocked_volume() {
    // A blocked strip x ∈ [−1, 1] across the whole floor splits it in two.
    let mut level = floor(10.0);
    level.area(Vec3::ZERO, Vec3::new(1.0, 2.0, 11.0), NavAreaKind::Blocked);
    assert_eq!(level.build().stats().islands, 2);
    // Grid cells `o` at x ∈ [−1, 1), z ∈ [0, 1) reopen a doorway (0.5 m wide after erosion).
    level.grid(-1.0, 0.0, 1.0, &["oo"]);
    let mesh = level.build();
    assert_eq!(mesh.stats().islands, 1);
    let path = mesh
        .path(Vec3::new(-5.0, 0.0, 0.5), Vec3::new(5.0, 0.0, 0.5))
        .unwrap();
    assert!(
        !path.partial && (path.length - 10.0).abs() < 1e-2,
        "{path:?}"
    );
}

/// Rooms and corridors (1 m cells); `#` is blocked. The rooms are big enough that no region is
/// dropped for being small.
const MAZE: [&str; 16] = [
    "########################",
    "#.....#......#.........#",
    "#.....#......#.........#",
    "#..........###....#....#",
    "#.....#......#....#....#",
    "###.###......#....######",
    "#.....####.###.........#",
    "#.....#.........####...#",
    "#.....#.........#..#...#",
    "#######.........#..#...#",
    "#.....#######.###..#####",
    "#.....#.....#......#...#",
    "#...........#......#...#",
    "#.....#.....#......#...#",
    "#.....#.....########...#",
    "########################",
];

/// 4-connected components of the grid's open cells, by flood fill.
fn grid_components(rows: &[&str]) -> Vec<Vec<Option<usize>>> {
    let (h, w) = (rows.len(), rows[0].len());
    let open = |x: usize, z: usize| rows[z].as_bytes()[x] != b'#';
    let mut label = vec![vec![None; w]; h];
    let mut next = 0;
    for z in 0..h {
        for x in 0..w {
            if !open(x, z) || label[z][x].is_some() {
                continue;
            }
            let mut stack = vec![(x, z)];
            label[z][x] = Some(next);
            while let Some((cx, cz)) = stack.pop() {
                let around = [
                    (cx.wrapping_sub(1), cz),
                    (cx + 1, cz),
                    (cx, cz.wrapping_sub(1)),
                    (cx, cz + 1),
                ];
                for (nx, nz) in around {
                    if nx < w && nz < h && open(nx, nz) && label[nz][nx].is_none() {
                        label[nz][nx] = Some(next);
                        stack.push((nx, nz));
                    }
                }
            }
            next += 1;
        }
    }
    label
}

#[test]
fn grid_walkability_reachability_equivalence_fixture() {
    let mut level = Level::new();
    level.slab(0.0, 0.0, 12.0, 8.0, 0.0);
    level.volume(Vec3::ZERO, volume_settings(Vec3::new(13.0, 3.0, 9.0)));
    level.grid(-12.0, -8.0, 1.0, &MAZE);
    let mesh = level.build();
    let labels = grid_components(&MAZE);
    let mut cells = Vec::new();
    for (z, row) in labels.iter().enumerate() {
        for (x, label) in row.iter().enumerate() {
            if let Some(label) = label {
                let center = Vec3::new(-12.0 + x as f32 + 0.5, 0.0, -8.0 + z as f32 + 0.5);
                let island = mesh
                    .island_at(center)
                    .unwrap_or_else(|| panic!("cell ({x}, {z}) is on the navmesh"));
                cells.push((*label, island));
            }
        }
    }
    let components = labels.iter().flatten().flatten().max().unwrap() + 1;
    assert_eq!(components, 3, "the fixture has three separate parts");
    assert_eq!(mesh.stats().islands, components);
    for (i, a) in cells.iter().enumerate() {
        for b in &cells[i + 1..] {
            assert_eq!(
                a.0 == b.0,
                a.1 == b.1,
                "cells {a:?} and {b:?}: grid and navmesh reachability differ"
            );
        }
    }
}

#[test]
fn large_level_tiles_without_silent_islands() {
    // 1 km of floor with a 0.6 m pillar every 8 m: 125 × 125 pillars around which polygons split.
    let mut level = Level::new();
    level.slab(0.0, 0.0, 500.0, 500.0, 0.0);
    let mut settings = volume_settings(Vec3::new(500.0, 3.0, 500.0));
    settings.cell_size = 0.5;
    settings.agent_radius = 0.5;
    level.volume(Vec3::ZERO, settings);
    for i in 0..125 {
        for j in 0..125 {
            let at = Vec3::new(-496.0 + 8.0 * i as f32, 0.0, -496.0 + 8.0 * j as f32);
            level.block(at, Vec3::new(0.3, 1.5, 0.3), Quat::IDENTITY);
        }
    }
    let mesh = level.build_with(8);
    let stats = mesh.stats();
    // 1000 m / (64 × 0.5 m) = 31.25, so 32 tiles a side, all with ground.
    assert_eq!(stats.tiles, 32 * 32);
    assert!(stats.polygons > 10_000, "{stats:?}");
    assert_eq!(
        stats.islands, 1,
        "every tile joins its neighbours: {stats:?}"
    );
    let corner = mesh
        .path(Vec3::new(-498.0, 0.0, -498.0), Vec3::new(498.0, 0.0, 498.0))
        .unwrap();
    assert!(!corner.partial, "the far corner is reachable");
    assert!(
        corner.length >= 996.0 * std::f32::consts::SQRT_2 - 1.0,
        "{}",
        corner.length
    );
}

#[test]
fn malformed_nav_settings_are_descriptive_errors() {
    let cases: Vec<(&str, NavMeshVolume)> = vec![
        (
            "half_extents",
            NavMeshVolume {
                half_extents: Vec3::new(1.0, 0.0, 1.0),
                ..NavMeshVolume::default()
            },
        ),
        (
            "cell_size",
            NavMeshVolume {
                cell_size: 0.0,
                ..NavMeshVolume::default()
            },
        ),
        (
            "cell_height",
            NavMeshVolume {
                cell_height: f32::NAN,
                ..NavMeshVolume::default()
            },
        ),
        (
            "agent_radius",
            NavMeshVolume {
                agent_radius: -0.1,
                ..NavMeshVolume::default()
            },
        ),
        (
            "agent_height",
            NavMeshVolume {
                agent_height: 0.0,
                ..NavMeshVolume::default()
            },
        ),
        (
            "agent_max_climb",
            NavMeshVolume {
                agent_max_climb: f32::INFINITY,
                ..NavMeshVolume::default()
            },
        ),
        (
            "agent_max_slope_deg",
            NavMeshVolume {
                agent_max_slope_deg: 90.0,
                ..NavMeshVolume::default()
            },
        ),
        (
            "tile_size",
            NavMeshVolume {
                tile_size: 4,
                ..NavMeshVolume::default()
            },
        ),
        (
            "edge_max_error",
            NavMeshVolume {
                edge_max_error: -1.0,
                ..NavMeshVolume::default()
            },
        ),
    ];
    for (field, volume) in cases {
        let mut level = Level::new();
        level.slab(0.0, 0.0, 5.0, 5.0, 0.0);
        level.volume(Vec3::ZERO, volume);
        let e = NavInput::gather(&level.world, level.volume.unwrap()).expect_err(field);
        assert!(
            matches!(&e, NavError::Invalid { message, .. } if message.contains(field)),
            "{field}: {e}"
        );
    }
    type AddMark = Box<dyn Fn(&mut Level)>;
    let marks: Vec<(&str, AddMark)> = vec![
        (
            "cost",
            Box::new(|l: &mut Level| {
                l.area(Vec3::ZERO, Vec3::ONE, NavAreaKind::Cost { cost: 0.5 });
            }),
        ),
        (
            "cost",
            Box::new(|l: &mut Level| {
                l.area(Vec3::ZERO, Vec3::ONE, NavAreaKind::Cost { cost: f32::NAN });
            }),
        ),
        (
            "half_extents",
            Box::new(|l: &mut Level| {
                l.area(Vec3::ZERO, Vec3::new(-1.0, 1.0, 1.0), NavAreaKind::Blocked);
            }),
        ),
        (
            "rows",
            Box::new(|l: &mut Level| {
                l.grid(0.0, 0.0, 1.0, &["..", "..."]);
            }),
        ),
        (
            "'x'",
            Box::new(|l: &mut Level| {
                l.grid(0.0, 0.0, 1.0, &[".x"]);
            }),
        ),
        (
            "cell_size",
            Box::new(|l: &mut Level| {
                l.grid(0.0, 0.0, 0.0, &[".."]);
            }),
        ),
    ];
    for (needle, add) in marks {
        let mut level = floor(5.0);
        add(&mut level);
        let e = NavInput::gather(&level.world, level.volume.unwrap()).expect_err(needle);
        assert!(
            matches!(&e, NavError::Invalid { message, .. } if message.contains(needle)),
            "{needle}: {e}"
        );
    }
    let mut level = floor(5.0);
    for i in 0..62 {
        level.area(
            Vec3::ZERO,
            Vec3::ONE,
            NavAreaKind::Cost {
                cost: 1.0 + i as f32,
            },
        );
    }
    assert_eq!(
        NavInput::gather(&level.world, level.volume.unwrap()),
        Err(NavError::TooManyCosts { count: 62, max: 61 })
    );
    let level = floor(5.0);
    let not_a_volume = level.world.entity_ids_sorted()[0];
    assert_eq!(
        NavInput::gather(&level.world, not_a_volume),
        Err(NavError::NoVolume(not_a_volume))
    );
}

#[test]
fn navmesh_data_roundtrips_bytes() {
    let data = build_tiles(&floor(10.0).input(), threads(2)).unwrap();
    let bytes = data.to_bytes();
    assert_eq!(NavMeshData::from_bytes(&bytes).unwrap(), data);
    for cut in [0, 3, 8, bytes.len() / 2, bytes.len() - 1] {
        assert!(
            matches!(
                NavMeshData::from_bytes(&bytes[..cut]),
                Err(NavError::Data(_))
            ),
            "cut at {cut}"
        );
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(
        matches!(NavMeshData::from_bytes(&extra), Err(NavError::Data(_))),
        "trailing bytes"
    );
}

#[test]
fn navmesh_is_cached_in_the_ddc() {
    let dir = tempfile::tempdir().unwrap();
    let cache = NavCache::new(Ddc::new(dir.path().join("ddc")));
    let mut level = floor(10.0);
    let first = cache.load_or_build(&level.input(), threads(2)).unwrap();
    assert!(!first.cached);
    let second = cache.load_or_build(&level.input(), threads(2)).unwrap();
    assert!(second.cached);
    assert_eq!(
        second.navmesh.data().to_bytes(),
        first.navmesh.data().to_bytes()
    );
    // Moving an obstacle in changes the key.
    level.block(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY);
    let changed = cache.load_or_build(&level.input(), threads(2)).unwrap();
    assert!(!changed.cached);
    assert_ne!(
        changed.navmesh.data().to_bytes(),
        first.navmesh.data().to_bytes()
    );
    // A corrupt entry is rebuilt, not trusted.
    let key = level.input().key().to_string();
    let path = dir.path().join("ddc").join(&key[..2]).join(&key);
    std::fs::write(&path, b"not a navmesh").unwrap();
    let rebuilt = cache.load_or_build(&level.input(), threads(2)).unwrap();
    assert!(!rebuilt.cached);
    assert_eq!(
        rebuilt.navmesh.data().to_bytes(),
        changed.navmesh.data().to_bytes()
    );
    assert!(
        cache
            .load_or_build(&level.input(), threads(2))
            .unwrap()
            .cached,
        "and stored again"
    );
}

/// Convex hull and compound colliders (M8 8.0a) are navmesh sources like the primitives: a hull
/// of a box's corners, or a compound of two such hulls, carves what the boxes carve.
#[test]
fn hull_and_compound_colliders_are_navmesh_sources() {
    let corners = |c: Vec3, h: Vec3| -> Vec<Vec3> {
        (0..8)
            .map(|i| {
                c + Vec3::new(
                    if i & 1 == 0 { -h.x } else { h.x },
                    if i & 2 == 0 { -h.y } else { h.y },
                    if i & 4 == 0 { -h.z } else { h.z },
                )
            })
            .collect()
    };
    // Faces mid-cell (±1.1), so float noise in the hull can't move them across a cell boundary.
    let half = Vec3::new(1.1, 1.0, 1.1);
    let area = |shape: Option<ColliderShape>| {
        let mut level = floor(10.0);
        match shape {
            Some(shape) => {
                level.body(
                    Vec3::new(0.0, 1.0, 0.0),
                    Quat::IDENTITY,
                    BodyKind::Static,
                    Collider::new(shape),
                );
            }
            None => {
                level.block(Vec3::ZERO, half, Quat::IDENTITY);
                level.block(Vec3::new(4.0, 0.0, 0.0), half, Quat::IDENTITY);
            }
        }
        ground_area(&level.build())
    };
    let boxes = area(None);
    let compound = area(Some(ColliderShape::Compound {
        parts: vec![
            corners(Vec3::ZERO, half),
            corners(Vec3::new(4.0, 0.0, 0.0), half),
        ],
    }));
    assert!(
        (boxes - compound).abs() < 0.01,
        "boxes {boxes}, compound {compound}"
    );
    let one_box = {
        let mut level = floor(10.0);
        level.block(Vec3::ZERO, half, Quat::IDENTITY);
        ground_area(&level.build())
    };
    let hull = area(Some(ColliderShape::ConvexHull {
        points: corners(Vec3::ZERO, half),
    }));
    assert!((one_box - hull).abs() < 0.01, "box {one_box}, hull {hull}");
    assert!(
        hull < ground_area(&floor(10.0).build()) - 1.0,
        "the hull carves the floor"
    );
}
