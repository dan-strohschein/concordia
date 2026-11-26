//! The canyon (M4 4.4c): a seeded canyon height function cut into heightfield tiles, a fixed set
//! of rock meshes, the layout that places them, and the two camera paths, all from fixed seeds.

use std::collections::BTreeSet;

use concordia_assets::MeshData;
use concordia_gen::canyon::{Canyon, Family, LENGTH, TILES, WIDTH};
use concordia_gen::mesh::{MeshRequest, build};

fn tile(tile: u32, cells: [u32; 2]) -> MeshData {
    build(
        &MeshRequest::CanyonTile {
            tile,
            of: TILES,
            cells,
        },
        Canyon::M4.seed,
    )
    .unwrap()
}

fn glb_hash(request: &MeshRequest, seed: u64) -> String {
    let bytes = concordia_gen::glb::write_glb(&build(request, seed).unwrap()).unwrap();
    blake3::hash(&bytes).to_hex().to_string()
}

/// A tile is a (cells + 1)² grid of vertices over its strip of the canyon, at the canyon's
/// heights, two triangles a cell; the floor is low and the walls high.
#[test]
fn canyon_tile_covers_its_strip_at_the_canyons_heights() {
    let m = tile(2, [32, 12]);
    assert_eq!(
        (m.positions.len(), m.indices.len() / 3),
        (33 * 13, 2 * 32 * 12)
    );
    let strip = LENGTH / TILES as f32;
    let (lo, hi) = m.aabb;
    let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
    assert!(
        close(lo[0], -WIDTH / 2.0) && close(hi[0], WIDTH / 2.0),
        "{:?}",
        m.aabb
    );
    assert!(
        close(hi[2], -2.0 * strip) && close(lo[2], -3.0 * strip),
        "{:?}",
        m.aabb
    );
    for p in &m.positions {
        assert_eq!(p[1], Canyon::M4.height(p[0], p[2]), "{p:?}");
    }
    let z = -2.5 * strip;
    let centre = Canyon::M4.centre(z);
    assert!(Canyon::M4.height(centre, z) < 2.0);
    assert!(Canyon::M4.height(WIDTH / 2.0 - 1.0, z) > 35.0);
    assert!(Canyon::M4.height(-WIDTH / 2.0 + 1.0, z) > 35.0);
}

/// Neighbouring tiles compute the same vertices (positions and normals, bit for bit) on their
/// shared edge, so the ground has no cracks.
#[test]
fn canyon_tiles_share_their_edges() {
    let cells = [32, 12];
    let edge = |m: &MeshData, z: f32| -> Vec<([u32; 3], [u32; 3])> {
        let mut v: Vec<_> = (0..m.positions.len())
            .filter(|&i| m.positions[i][2] == z)
            .map(|i| {
                (
                    m.positions[i].map(f32::to_bits),
                    m.normals[i].map(f32::to_bits),
                )
            })
            .collect();
        v.sort();
        v
    };
    for t in 0..TILES - 1 {
        let (a, b) = (tile(t, cells), tile(t + 1, cells));
        let z = a.aabb.0[2];
        assert_eq!(
            z,
            b.aabb.1[2],
            "tile {t}'s far edge is tile {}'s near edge",
            t + 1
        );
        let (ea, eb) = (edge(&a, z), edge(&b, z));
        assert_eq!(ea.len(), 33);
        assert_eq!(ea, eb, "tiles {t} and {}", t + 1);
    }
}

/// Bad tile requests name the field.
#[test]
fn canyon_tile_requests_are_validated() {
    for (req, field) in [
        (
            MeshRequest::CanyonTile {
                tile: 8,
                of: 8,
                cells: [4, 4],
            },
            "tile",
        ),
        (
            MeshRequest::CanyonTile {
                tile: 0,
                of: 0,
                cells: [4, 4],
            },
            "of",
        ),
        (
            MeshRequest::CanyonTile {
                tile: 0,
                of: 8,
                cells: [0, 4],
            },
            "cells",
        ),
        (
            MeshRequest::CanyonTile {
                tile: 0,
                of: 8,
                cells: [4, 5000],
            },
            "cells",
        ),
    ] {
        let err = build(&req, 1).unwrap_err().to_string();
        assert!(err.starts_with(field), "{req:?}: {err}");
    }
}

/// The canyon's assets: eight tiles and four rocks of each family, each a path, a request and a
/// seed; every rock seed differs.
#[test]
fn canyon_assets_are_eight_tiles_and_twelve_rocks() {
    let assets = Canyon::M4.assets();
    let tiles = assets
        .iter()
        .filter(|a| matches!(a.request, MeshRequest::CanyonTile { .. }))
        .count();
    assert_eq!(tiles, TILES as usize);
    for family in [Family::Cliff, Family::Boulder, Family::Stone] {
        assert_eq!(
            assets.iter().filter(|a| a.family == Some(family)).count(),
            4
        );
    }
    let paths: BTreeSet<_> = assets.iter().map(|a| a.path.as_str()).collect();
    assert_eq!(paths.len(), assets.len());
    assert!(
        paths
            .iter()
            .all(|p| p.starts_with("assets/") && p.ends_with(".glb"))
    );
    let seeds: BTreeSet<_> = assets
        .iter()
        .filter(|a| a.family.is_some())
        .map(|a| a.seed)
        .collect();
    assert_eq!(seeds.len(), 12);
}

/// The layout is the same every time: each tile once, 52 cliff blocks, 80 boulders and 180
/// stones, each with a unique name, one of the canyon's assets, a yaw-only unit rotation, a
/// uniform scale of 0.7–1.4, inside the canyon and resting on (or set into) the ground.
#[test]
fn canyon_layout_is_deterministic_and_uniformly_scaled() {
    let canyon = Canyon::M4;
    let layout = canyon.layout();
    assert_eq!(layout, canyon.layout());
    let assets = canyon.assets();
    let family = |path: &str| assets.iter().find(|a| a.path == path).map(|a| a.family);
    let count = |f: Option<Family>| {
        layout
            .iter()
            .filter(|p| family(&p.asset) == Some(f))
            .count()
    };
    assert_eq!(
        (
            count(None),
            count(Some(Family::Cliff)),
            count(Some(Family::Boulder)),
            count(Some(Family::Stone))
        ),
        (8, 52, 80, 180)
    );
    let names: BTreeSet<_> = layout.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names.len(), layout.len());
    for p in &layout {
        assert!(
            family(&p.asset).is_some(),
            "{} uses an unknown asset",
            p.name
        );
        let [x, y, z, w] = p.rotation;
        assert_eq!((x, z), (0.0, 0.0), "{}: yaw only", p.name);
        assert!((y * y + w * w - 1.0).abs() < 1e-5, "{}: unit", p.name);
        if family(&p.asset) == Some(None) {
            assert_eq!(
                (p.translation, p.scale, p.rotation),
                ([0.0; 3], 1.0, [0.0, 0.0, 0.0, 1.0])
            );
            continue;
        }
        assert!((0.7..=1.4).contains(&p.scale), "{}: {}", p.name, p.scale);
        let [tx, ty, tz] = p.translation;
        assert!(
            tx.abs() < WIDTH / 2.0 && (-LENGTH..0.0).contains(&tz),
            "{}: {:?}",
            p.name,
            p.translation
        );
        let ground = canyon.height(tx, tz);
        assert!(
            (ty - ground).abs() < 12.0,
            "{}: {ty} vs ground {ground}",
            p.name
        );
        assert!(p.material.starts_with("materials/") && p.material.ends_with(".material.ron"));
    }
}

/// The ground path flies 2.5 m over the floor down the canyon's centreline, looking 30 m ahead;
/// the overhead path looks down on the centreline from high up. Both repeat exactly.
#[test]
fn canyon_camera_paths_follow_the_centreline() {
    let canyon = Canyon::M4;
    let ground = canyon.ground_path();
    assert_eq!(ground.len(), 12);
    assert_eq!(ground, canyon.ground_path());
    for k in &ground {
        let [x, y, z] = k.eye;
        assert!((x - canyon.centre(z)).abs() < 1e-4, "{k:?}");
        assert!((y - canyon.height(x, z) - 2.5).abs() < 1e-4, "{k:?}");
        assert!((k.target[2] - (z - 30.0)).abs() < 1e-4, "{k:?}");
        assert!((-LENGTH..0.0).contains(&z) && (-LENGTH..0.0).contains(&k.target[2]));
    }
    assert!(
        ground.windows(2).all(|w| w[1].eye[2] < w[0].eye[2]),
        "moves down the canyon"
    );
    let overhead = canyon.overhead_path();
    assert_eq!(overhead.len(), 6);
    for k in &overhead {
        assert!(k.eye[1] - k.target[1] > 50.0, "{k:?}");
        assert!(
            (k.target[0] - canyon.centre(k.target[2])).abs() < 1e-4,
            "{k:?}"
        );
    }
}

/// The generators use only arithmetic, `sqrt`, `floor` and integer hashing, so their bytes are
/// pinned and equal on macOS and Linux CI (a small rock and a small tile).
#[test]
fn generators_use_no_platform_maths() {
    let rock = MeshRequest::Rock {
        size: [2.0, 1.5, 1.8],
        detail: 36,
        roughness: 0.25,
        strata: 0.3,
        lumps: 5,
    };
    let tile = MeshRequest::CanyonTile {
        tile: 3,
        of: TILES,
        cells: [40, 16],
    };
    assert_eq!(
        glb_hash(&rock, 77),
        "acb403c29ad9f0890f3de6c835945b01ba25621b4fdb48960eda992ac0343cab"
    );
    assert_eq!(
        glb_hash(&tile, Canyon::M4.seed),
        "08859ea1e5ee016af5dcb567baeea91c29eb2725c7b0597c967f8990862bbf7c"
    );
}
