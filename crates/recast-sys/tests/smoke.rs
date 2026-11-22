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
    let tile = build_tile(&config(), &plane_input()).unwrap().unwrap();
    let mut mesh = DtNavMesh::new(&params()).unwrap();
    mesh.add_tile(&tile).unwrap();
    assert_eq!(
        mesh.add_tile(&tile),
        Err(RecastError::AddTile),
        "the slot is taken"
    );
}
