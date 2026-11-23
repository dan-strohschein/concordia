//! recast-sys: the shim's ABI, a flat-plane build, input validation and released handles.
//!
//! Expected values (cell 0.25 m, cell height 0.2 m, radius 0.5 m) for a 10 m plane at y = 0:
//! - `rcFilterLedgeSpans` removes the outer ring of cells (each is next to a drop with no floor),
//!   then erosion removes ceil(0.5 / 0.25) = 2 more: 3 cells = 0.75 m, so the one region is the
//!   square ±4.25 m. Its contour keeps the four corners (edges of 34 cells are under the 48-cell
//!   `edge_max_len` of 12 m), which `rcBuildPolyMesh` merges into one quad.
//! - Heights: from a tile floor of −1 m the plane is at 1 / 0.2 = cell 5, and a span is at least
//!   one cell tall, so its top (the walkable surface) is cell 6: y = −1 + 6 × 0.2 = 0.2.

use std::sync::Arc;

use recast_sys::{
    AreaCosts, BuildConfig, DtNavMesh, DtQuery, NavMeshParams, RecastError, TileData, TileInput,
    build_tile, live_allocations,
};

/// The plane's walkable surface (see the header).
const SURFACE: f32 = 0.2;

fn config() -> BuildConfig {
    BuildConfig {
        cell_size: 0.25,
        cell_height: 0.2,
        agent_height: 1.8,
        agent_radius: 0.5,
        agent_max_climb: 0.3,
        agent_max_slope_deg: 45.0,
        tile_size: 40,
        region_min_size: 8,
        region_merge_size: 20,
        edge_max_len: 12.0,
        edge_max_error: 1.3,
        verts_per_poly: 6,
        detail_sample_dist: 1.5,
        detail_sample_max_error: 0.2,
    }
}

const PLANE: [[f32; 3]; 4] = [
    [-5.0, 0.0, -5.0],
    [5.0, 0.0, -5.0],
    [5.0, 0.0, 5.0],
    [-5.0, 0.0, 5.0],
];
/// Counter-clockwise seen from above, so the normals point up.
const UP: [[u32; 3]; 2] = [[0, 2, 1], [0, 3, 2]];

fn plane_input<'a>() -> TileInput<'a> {
    TileInput {
        vertices: &PLANE,
        triangles: &UP,
        marks: &[],
        tile: [0, 0],
        min: [-5.0, -1.0, -5.0],
        max: [5.0, 1.0, 5.0],
    }
}

fn params() -> NavMeshParams {
    NavMeshParams {
        origin: [-5.0, 0.0, -5.0],
        tile_width: 10.0,
        tile_depth: 10.0,
        max_tiles: 4,
        max_polys_per_tile: 256,
    }
}

fn plane_mesh() -> Arc<DtNavMesh> {
    let tile = build_tile(&config(), &plane_input())
        .expect("builds")
        .expect("not empty");
    let mut mesh = DtNavMesh::new(&params()).expect("navmesh");
    mesh.add_tile(&tile).expect("adds");
    Arc::new(mesh)
}

#[test]
fn header_hash_and_struct_sizes_match() {
    let header = include_bytes!("../shim/concordia_recast.h");
    let hash = blake3::hash(header).to_hex().to_string();
    let bindings = include_str!("../src/ffi.rs");
    assert!(
        bindings.contains(&format!("// header blake3: {hash}")),
        "src/ffi.rs must mirror the header; update its hash line to {hash}"
    );
    assert_eq!(recast_sys::abi_mismatches(), Vec::<String>::new());
}

#[test]
fn recast_builds_navmesh_for_flat_plane() {
    let mesh = plane_mesh();
    let tiles = mesh.tiles();
    assert_eq!(tiles.len(), 1, "{tiles:?}");
    assert_eq!(tiles[0].tile, [0, 0]);
    let polys = mesh.polys();
    assert_eq!(
        polys.len(),
        1,
        "one eroded square region is one quad: {polys:?}"
    );
    let quad = &polys[0];
    assert_eq!(quad.vertices.len(), 4);
    assert_eq!(quad.area, recast_sys::Area::WALKABLE);
    for v in &quad.vertices {
        assert!((v[0].abs() - 4.25).abs() < 1e-3, "x on ±4.25: {v:?}");
        assert!((v[2].abs() - 4.25).abs() < 1e-3, "z on ±4.25: {v:?}");
        assert!(
            (v[1] - SURFACE).abs() < 1e-4,
            "y on the voxel surface: {v:?}"
        );
    }
    let corners: std::collections::BTreeSet<(i32, i32)> = quad
        .vertices
        .iter()
        .map(|v| (v[0].signum() as i32, v[2].signum() as i32))
        .collect();
    assert_eq!(corners.len(), 4, "all four corners: {:?}", quad.vertices);
    assert_eq!(
        mesh.links(quad.reference).expect("links"),
        vec![],
        "alone in its tile"
    );
}

#[test]
fn queries_work_on_the_plane() {
    let mesh = plane_mesh();
    let mut query = DtQuery::new(mesh, 256).expect("query");
    let costs = AreaCosts::default();
    let (poly, snapped) = query
        .nearest(&costs, [1.0, 2.0, -1.0], [0.5, 3.0, 0.5])
        .expect("query")
        .expect("over the mesh");
    assert!(
        (snapped[0] - 1.0).abs() < 1e-4
            && (snapped[2] + 1.0).abs() < 1e-4
            && (snapped[1] - SURFACE).abs() < 1e-4,
        "{snapped:?}"
    );
    assert_eq!(
        query
            .nearest(&costs, [40.0, 0.0, 0.0], [0.5, 1.0, 0.5])
            .expect("query"),
        None
    );
    let (end_poly, end) = query
        .nearest(&costs, [-3.0, 0.0, 3.0], [0.5, 1.0, 0.5])
        .unwrap()
        .unwrap();
    let corridor = query
        .find_path(&costs, (poly, snapped), (end_poly, end), 64)
        .expect("path");
    assert_eq!(
        (corridor.polys.as_slice(), corridor.partial),
        ([poly].as_slice(), false)
    );
    let points = query
        .straight_path(snapped, end, &corridor.polys, 16)
        .expect("straight");
    assert_eq!(points.len(), 2, "open floor is one segment: {points:?}");
    // From x = 1 toward x = 10 the edge is at x = 4.25: t = 3.25 / 9.
    let ray = query
        .raycast(&costs, poly, snapped, [10.0, 0.0, -1.0])
        .expect("ray");
    let t = ray.hit.expect("hits the edge");
    assert!((t - 3.25 / 9.0).abs() < 1e-4, "t = {t}");
    assert!(
        (ray.normal[0] + 1.0).abs() < 1e-3,
        "faces back toward −x: {:?}",
        ray.normal
    );
    let free = query
        .raycast(&costs, poly, snapped, [2.0, 0.0, -1.0])
        .expect("ray");
    assert_eq!(free.hit, None);
}

#[test]
fn ffi_handles_are_released() {
    let before = live_allocations();
    {
        let mesh = plane_mesh();
        let mut query = DtQuery::new(mesh.clone(), 512).expect("query");
        let costs = AreaCosts::default();
        let start = query
            .nearest(&costs, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0])
            .unwrap()
            .unwrap();
        let _ = query.find_path(&costs, start, start, 8).expect("path");
        let _ = query
            .move_along_surface(&costs, start.0, start.1, [3.0, 0.2, 0.0], 16)
            .expect("move");
        let _ = query.poly_height(start.0, start.1).expect("height");
        let _ = query
            .raycast_path(&costs, start.0, start.1, [9.0, 0.2, 0.0], 32)
            .expect("ray");
        let _ = query
            .local_neighbourhood(&costs, start.0, start.1, 2.0, 16)
            .expect("neighbourhood");
        let _ = query.wall_segments(&costs, start.0, 18).expect("walls");
        assert!(
            live_allocations() > before,
            "the navmesh and query are counted"
        );
        drop(mesh);
        assert!(
            live_allocations() > before,
            "the query still holds the navmesh"
        );
    }
    assert_eq!(
        live_allocations(),
        before,
        "dropping the query frees the navmesh"
    );
    let mut bad = config();
    bad.cell_size = 0.0;
    assert!(build_tile(&bad, &plane_input()).is_err());
    let garbage = TileData::from_bytes(vec![7; 64]);
    let mut mesh = DtNavMesh::new(&params()).expect("navmesh");
    assert!(mesh.add_tile(&garbage).is_err());
    drop(mesh);
    assert_eq!(
        live_allocations(),
        before,
        "failed builds and refused tiles leak nothing"
    );
}

#[test]
fn shim_rejects_bad_input() {
    let input = |e: RecastError| matches!(e, RecastError::Input(_));
    for broken in [
        BuildConfig {
            cell_size: 0.0,
            ..config()
        },
        BuildConfig {
            cell_height: -0.2,
            ..config()
        },
        BuildConfig {
            agent_radius: f32::NAN,
            ..config()
        },
        BuildConfig {
            agent_height: 0.0,
            ..config()
        },
        BuildConfig {
            agent_max_slope_deg: 90.0,
            ..config()
        },
        BuildConfig {
            tile_size: 0,
            ..config()
        },
        BuildConfig {
            verts_per_poly: 7,
            ..config()
        },
        BuildConfig {
            verts_per_poly: 2,
            ..config()
        },
        BuildConfig {
            edge_max_error: f32::INFINITY,
            ..config()
        },
    ] {
        let e = build_tile(&broken, &plane_input()).expect_err(&format!("{broken:?}"));
        assert!(input(e.clone()), "{broken:?}: {e}");
    }
    let empty = TileInput {
        triangles: &[],
        ..plane_input()
    };
    assert_eq!(
        build_tile(&config(), &empty),
        Ok(None),
        "no triangles is an empty tile"
    );
    let out_of_range = TileInput {
        triangles: &[[0, 1, 9]],
        ..plane_input()
    };
    assert!(input(build_tile(&config(), &out_of_range).unwrap_err()));
    let nan = [[f32::NAN, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let nan_input = TileInput {
        vertices: &nan,
        triangles: &[[0, 2, 1]],
        ..plane_input()
    };
    assert!(input(build_tile(&config(), &nan_input).unwrap_err()));
    let flat = TileInput {
        max: [5.0, -1.0, 5.0],
        ..plane_input()
    };
    assert!(
        input(build_tile(&config(), &flat).unwrap_err()),
        "empty tile bounds"
    );
    for p in [
        NavMeshParams {
            max_tiles: 0,
            ..params()
        },
        NavMeshParams {
            tile_width: -1.0,
            ..params()
        },
        NavMeshParams {
            max_polys_per_tile: 0,
            ..params()
        },
        NavMeshParams {
            origin: [f32::NAN, 0.0, 0.0],
            ..params()
        },
    ] {
        assert!(input(DtNavMesh::new(&p).unwrap_err()), "{p:?}");
    }
    assert!(input(DtQuery::new(plane_mesh(), 0).unwrap_err()));
    let mut costs = AreaCosts::default();
    assert!(
        costs.set(64, 2.0).is_err()
            && costs.set(3, 0.5).is_err()
            && costs.set(3, f32::NAN).is_err()
    );
    let mut query = DtQuery::new(plane_mesh(), 64).unwrap();
    let costs = AreaCosts::default();
    let (poly, p) = query
        .nearest(&costs, [0.0, 0.2, 0.0], [0.5, 1.0, 0.5])
        .unwrap()
        .unwrap();
    let nan = [f32::NAN, 0.0, 0.0];
    let stale = recast_sys::PolyRef(std::num::NonZeroU64::new(0xdead_beef).unwrap());
    assert!(input(
        query
            .move_along_surface(&costs, poly, nan, p, 16)
            .unwrap_err()
    ));
    assert!(
        input(query.move_along_surface(&costs, poly, p, p, 0).unwrap_err()),
        "no visited slots"
    );
    assert_eq!(
        query.move_along_surface(&costs, stale, p, p, 16),
        Err(RecastError::NotFound)
    );
    assert!(input(query.poly_height(poly, nan).unwrap_err()));
    assert_eq!(query.poly_height(stale, p), Err(RecastError::NotFound));
    assert!(input(
        query.raycast_path(&costs, poly, p, nan, 32).unwrap_err()
    ));
    assert!(
        input(query.raycast_path(&costs, poly, p, p, 0).unwrap_err()),
        "no path slots"
    );
    assert_eq!(
        query.raycast_path(&costs, stale, p, p, 32),
        Err(RecastError::NotFound)
    );
    assert!(input(
        query
            .local_neighbourhood(&costs, poly, nan, 1.0, 16)
            .unwrap_err()
    ));
    for radius in [f32::NAN, -1.0] {
        assert!(
            input(
                query
                    .local_neighbourhood(&costs, poly, p, radius, 16)
                    .unwrap_err()
            ),
            "radius {radius}"
        );
    }
    assert!(
        input(
            query
                .local_neighbourhood(&costs, poly, p, 1.0, 0)
                .unwrap_err()
        ),
        "no polygon slots"
    );
    assert_eq!(
        query.local_neighbourhood(&costs, stale, p, 1.0, 16),
        Err(RecastError::NotFound)
    );
    assert!(
        input(query.wall_segments(&costs, poly, 0).unwrap_err()),
        "no segment slots"
    );
    assert_eq!(
        query.wall_segments(&costs, stale, 18),
        Err(RecastError::NotFound)
    );
    let tile = build_tile(&config(), &plane_input()).unwrap().unwrap();
    let mut mesh = DtNavMesh::new(&params()).unwrap();
    mesh.add_tile(&tile).unwrap();
    assert_eq!(
        mesh.add_tile(&tile),
        Err(RecastError::AddTile),
        "the slot is taken"
    );
}

// Crowd queries (2.5c.1). A 20 × 10 m plane (x −5..15, z −5..5) cut into two 10 m tiles at x = 5:
// - outer edges lose 0.75 m as on the single plane (ledge ring + 2 eroded cells), so the walkable
//   floor is x −4.25..14.25, z ±4.25, at y = 0.2;
// - the seam at x = 5 isn't an edge: each tile's border reaches into the other's geometry, so
//   nothing is eroded there and the two tiles link across it.

const LONG_PLANE: [[f32; 3]; 4] = [
    [-5.0, 0.0, -5.0],
    [15.0, 0.0, -5.0],
    [15.0, 0.0, 5.0],
    [-5.0, 0.0, 5.0],
];

fn two_tile_mesh() -> Arc<DtNavMesh> {
    let mut mesh = DtNavMesh::new(&params()).expect("navmesh");
    for (x, min_x) in [(0, -5.0), (1, 5.0)] {
        let input = TileInput {
            vertices: &LONG_PLANE,
            triangles: &UP,
            marks: &[],
            tile: [x, 0],
            min: [min_x, -1.0, -5.0],
            max: [min_x + 10.0, 1.0, 5.0],
        };
        let tile = build_tile(&config(), &input)
            .expect("builds")
            .expect("not empty");
        mesh.add_tile(&tile).expect("adds");
    }
    Arc::new(mesh)
}

fn snap(query: &mut DtQuery, p: [f32; 3]) -> (recast_sys::PolyRef, [f32; 3]) {
    query
        .nearest(&AreaCosts::default(), p, [0.5, 1.0, 0.5])
        .expect("query")
        .expect("on the floor")
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
}

#[test]
fn move_along_surface_slides_and_stops_at_edges() {
    let mut query = DtQuery::new(two_tile_mesh(), 256).expect("query");
    let costs = AreaCosts::default();
    let (a, start) = snap(&mut query, [0.0, 0.2, 0.0]);
    let (b, _) = snap(&mut query, [8.0, 0.2, 0.0]);
    assert_ne!(a, b, "one polygon per tile side of the seam");
    // Across the seam: the end is reached, through both polygons, the start's first.
    let moved = query
        .move_along_surface(&costs, a, start, [8.0, 0.2, 0.0], 16)
        .expect("moves");
    assert!(close(moved.position, [8.0, 0.2, 0.0]), "{moved:?}");
    assert_eq!(moved.visited, vec![a, b]);
    // Past the far edge: it stops at x = 14.25.
    let stopped = query
        .move_along_surface(&costs, a, start, [20.0, 0.2, 0.0], 16)
        .expect("moves");
    assert!(close(stopped.position, [14.25, 0.2, 0.0]), "{stopped:?}");
    // Into a side wall at a slant: it slides along z = 4.25 instead of stopping (x keeps going).
    let slid = query
        .move_along_surface(&costs, a, start, [2.0, 0.2, 6.0], 16)
        .expect("moves");
    assert!(close(slid.position, [2.0, 0.2, 4.25]), "{slid:?}");
    assert_eq!(slid.visited, vec![a]);
    // One visited slot keeps only the start's polygon.
    let short = query
        .move_along_surface(&costs, a, start, [8.0, 0.2, 0.0], 1)
        .expect("moves");
    assert_eq!(short.visited.len(), 1, "{short:?}");
}

#[test]
fn poly_height_reads_the_surface() {
    let mut query = DtQuery::new(two_tile_mesh(), 256).expect("query");
    let (b, _) = snap(&mut query, [8.0, 0.2, 0.0]);
    let h = query.poly_height(b, [8.0, 5.0, 1.0]).expect("query");
    assert!(h.is_some_and(|h| (h - SURFACE).abs() < 1e-4), "{h:?}");
    assert_eq!(
        query.poly_height(b, [0.0, 0.0, 0.0]).expect("query"),
        None,
        "x = 0 is in the other tile's polygon"
    );
}

#[test]
fn raycast_path_lists_visited_polygons() {
    let mut query = DtQuery::new(two_tile_mesh(), 256).expect("query");
    let costs = AreaCosts::default();
    let (a, start) = snap(&mut query, [0.0, 0.2, 0.0]);
    let (b, _) = snap(&mut query, [8.0, 0.2, 0.0]);
    let free = query
        .raycast_path(&costs, a, start, [8.0, 0.2, 0.0], 32)
        .expect("ray");
    assert_eq!((free.ray.hit, free.polys.clone()), (None, vec![a, b]));
    // From x = 0 toward x = 20 the far edge is at 14.25: t = 14.25 / 20.
    let hit = query
        .raycast_path(&costs, a, start, [20.0, 0.2, 0.0], 32)
        .expect("ray");
    let t = hit.ray.hit.expect("hits the far edge");
    assert!((t - 14.25 / 20.0).abs() < 1e-4, "t = {t}");
    assert_eq!(hit.polys, vec![a, b]);
    assert_eq!(
        query
            .raycast(&costs, a, start, [20.0, 0.2, 0.0])
            .expect("ray")
            .hit,
        hit.ray.hit,
        "the same ray as cr_raycast"
    );
}

// Boundary queries (2.5c.2), on the same two-tile plane: polygon `a` covers x −4.25..5 and `b`
// covers 5..14.25 (z ±4.25), linked across the seam at x = 5.

#[test]
fn local_neighbourhood_grows_with_its_radius() {
    // From (0, 0.2, 0) the seam is 5 m away: a radius of 4.9 m stays in `a`, and 6 m reaches `b`
    // through the portal (Detour adds a polygon whose shared edge is within the radius).
    let mut query = DtQuery::new(two_tile_mesh(), 256).expect("query");
    let costs = AreaCosts::default();
    let (a, start) = snap(&mut query, [0.0, 0.2, 0.0]);
    let (b, _) = snap(&mut query, [8.0, 0.2, 0.0]);
    let near = query
        .local_neighbourhood(&costs, a, start, 4.9, 16)
        .expect("query");
    assert_eq!(near, vec![a]);
    let far = query
        .local_neighbourhood(&costs, a, start, 6.0, 16)
        .expect("query");
    assert_eq!(far, vec![a, b]);
    let one = query
        .local_neighbourhood(&costs, a, start, 6.0, 1)
        .expect("query");
    assert_eq!(one, vec![a], "one slot keeps the start");
}

#[test]
fn wall_segments_are_the_unlinked_edges() {
    // `a`'s outline is x −4.25..5 by z ±4.25. Its edges at z = ±4.25 (9.25 m each) and x = −4.25
    // (8.5 m) are walls, 27 m in all; its seam edge at x = 5 links to `b`, so it isn't one.
    let mut query = DtQuery::new(two_tile_mesh(), 256).expect("query");
    let costs = AreaCosts::default();
    let (a, _) = snap(&mut query, [0.0, 0.2, 0.0]);
    let walls = query.wall_segments(&costs, a, 18).expect("query");
    let length: f32 = walls
        .iter()
        .map(|[p, q]| ((p[0] - q[0]).powi(2) + (p[2] - q[2]).powi(2)).sqrt())
        .sum();
    assert!((length - 27.0).abs() < 1e-3, "{length}: {walls:?}");
    let on_seam = |s: &[[f32; 3]; 2]| (s[0][0] - 5.0).abs() < 1e-3 && (s[1][0] - 5.0).abs() < 1e-3;
    assert!(!walls.iter().any(on_seam), "{walls:?}");
    let first = query.wall_segments(&costs, a, 1).expect("query");
    assert_eq!(first.len(), 1, "capped: {first:?}");
}
