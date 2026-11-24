//! The procedural mesh provider (M3 phase 3.7a): shapes with hand-derived counts and bounds,
//! closed shapes that are watertight with outward winding, seeded determinism and GLB bytes.

use std::collections::BTreeMap;

use concordia_assets::MeshData;
use concordia_gen::mesh::{MeshRequest, SdfShape, build};

/// Positions welded to 1e-4 m, so seams and poles join.
fn weld(p: [f32; 3]) -> [i64; 3] {
    p.map(|c| (c * 1e4).round() as i64)
}

/// Every edge of every non-degenerate triangle is shared by exactly two triangles walking it in
/// opposite directions (closed and consistently wound), and the signed volume is positive
/// (outward), close to `volume`.
fn assert_closed(name: &str, m: &MeshData, volume: f64, tolerance: f64) {
    let mut edges: BTreeMap<([i64; 3], [i64; 3]), i32> = BTreeMap::new();
    let mut signed = 0.0f64;
    for t in m.indices.chunks(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| m.positions[i as usize]);
        let [wa, wb, wc] = [a, b, c].map(weld);
        if wa == wb || wb == wc || wc == wa {
            continue;
        }
        for (u, v) in [(wa, wb), (wb, wc), (wc, wa)] {
            *edges.entry((u, v)).or_default() += 1;
        }
        let [a, b, c] = [a, b, c].map(|p| p.map(f64::from));
        signed += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    for ((u, v), n) in &edges {
        assert_eq!(*n, 1, "{name}: edge {u:?}->{v:?} walked {n} times");
        assert_eq!(
            edges.get(&(*v, *u)),
            Some(&1),
            "{name}: edge {u:?}->{v:?} has no twin"
        );
    }
    assert!(
        (signed - volume).abs() <= tolerance * volume,
        "{name}: volume {signed}, want {volume}"
    );
}

fn bounds(m: &MeshData) -> ([f32; 3], [f32; 3]) {
    m.aabb
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
}

/// A 1 m box has 4 vertices per face (flat normals): 24 vertices, 12 triangles, bounds ±0.5.
#[test]
fn box_mesh_has_24_vertices_12_triangles_and_unit_bounds() {
    let m = build(
        &MeshRequest::Box {
            size: [1.0, 1.0, 1.0],
        },
        0,
    )
    .unwrap();
    assert_eq!((m.positions.len(), m.indices.len() / 3), (24, 12));
    assert!(
        close(bounds(&m).0, [-0.5; 3]) && close(bounds(&m).1, [0.5; 3]),
        "{:?}",
        m.aabb
    );
    let tall = build(
        &MeshRequest::Box {
            size: [2.0, 4.0, 1.0],
        },
        0,
    )
    .unwrap();
    assert!(close(tall.aabb.1, [1.0, 2.0, 0.5]));
    assert_closed("box", &m, 1.0, 1e-4);
}

/// Closed shapes are watertight and outward, with volumes near the solids they approximate:
/// - sphere r = 1 (64 × 32): 4/3 π ≈ 4.18879, within 1%;
/// - capsule r = 0.5, cylinder part 1 m: π 0.25 · 1 + 4/3 π 0.125 ≈ 1.30900, within 1%;
/// - cylinder r = 0.5, h = 2 (64 sides): π 0.25 · 2 ≈ 1.57080, within 1%;
/// - torus R = 1, r = 0.25 (64 × 32): 2 π² R r² ≈ 1.23370, within 1%;
/// - lathe of a 1 × 1 rectangle profile ((0,0) (1,0) (1,1) (0,1)), 64 steps: a cylinder of
///   volume π ≈ 3.14159, within 1%;
/// - extrusion of a unit square by 2 m: exactly 2;
/// - SDF sphere r = 1 at 0.1 m cells: ≈ 4.18879 within 5%.
#[test]
fn generated_mesh_is_watertight_and_has_consistent_winding() {
    use std::f64::consts::PI;
    let cases: Vec<(&str, MeshRequest, f64, f64)> = vec![
        (
            "sphere",
            MeshRequest::Sphere {
                radius: 1.0,
                segments: 64,
                rings: 32,
            },
            4.0 / 3.0 * PI,
            0.01,
        ),
        (
            "capsule",
            MeshRequest::Capsule {
                radius: 0.5,
                height: 1.0,
                segments: 64,
                rings: 16,
            },
            PI * 0.25 + 4.0 / 3.0 * PI * 0.125,
            0.01,
        ),
        (
            "cylinder",
            MeshRequest::Cylinder {
                radius: 0.5,
                height: 2.0,
                segments: 64,
            },
            PI * 0.5,
            0.01,
        ),
        (
            "torus",
            MeshRequest::Torus {
                major: 1.0,
                minor: 0.25,
                segments: 64,
                sides: 32,
            },
            2.0 * PI * PI * 0.0625,
            0.01,
        ),
        (
            "lathe",
            MeshRequest::Lathe {
                profile: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                segments: 64,
            },
            PI,
            0.01,
        ),
        (
            "extrude",
            MeshRequest::Extrude {
                profile: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                depth: 2.0,
            },
            2.0,
            1e-4,
        ),
        (
            "sdf",
            MeshRequest::Sdf {
                shapes: vec![SdfShape::Sphere {
                    center: [0.0; 3],
                    radius: 1.0,
                }],
                smooth: 0.0,
                cell: 0.1,
            },
            4.0 / 3.0 * PI,
            0.05,
        ),
    ];
    for (name, request, volume, tolerance) in cases {
        let m = build(&request, 0).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(m.normals.len(), m.positions.len(), "{name}");
        assert_eq!(m.uv0.len(), m.positions.len(), "{name}");
        assert_closed(name, &m, volume, tolerance);
    }
}

/// A capsule of radius 0.5 and cylinder part 1 m stands 2 m tall: y from −1 to 1. A torus of
/// R 1, r 0.25 spans ±1.25 in x and z and ±0.25 in y. A 10 × 4 m heightfield of 20 × 8 cells
/// has 21 × 9 vertices and 2 · 160 triangles, within ±amplitude.
#[test]
fn shapes_have_their_sizes() {
    let c = build(
        &MeshRequest::Capsule {
            radius: 0.5,
            height: 1.0,
            segments: 16,
            rings: 8,
        },
        0,
    )
    .unwrap();
    assert!(
        (c.aabb.0[1] + 1.0).abs() < 1e-4 && (c.aabb.1[1] - 1.0).abs() < 1e-4,
        "{:?}",
        c.aabb
    );
    let t = build(
        &MeshRequest::Torus {
            major: 1.0,
            minor: 0.25,
            segments: 64,
            sides: 16,
        },
        0,
    )
    .unwrap();
    assert!(close(t.aabb.1, [1.25, 0.25, 1.25]), "{:?}", t.aabb);
    let h = build(
        &MeshRequest::Heightfield {
            size: [10.0, 4.0],
            cells: [20, 8],
            amplitude: 0.5,
            frequency: 1.0,
        },
        7,
    )
    .unwrap();
    assert_eq!((h.positions.len(), h.indices.len() / 3), (21 * 9, 320));
    assert!(h.aabb.0[1] >= -0.5 && h.aabb.1[1] <= 0.5, "{:?}", h.aabb);
    assert!(close([h.aabb.0[0], 0.0, h.aabb.0[2]], [-5.0, 0.0, -2.0]));
}

/// The same request and seed give the same bytes; a seeded shape changes with the seed.
#[test]
fn generation_is_deterministic_for_a_seed() {
    let req = MeshRequest::Heightfield {
        size: [4.0, 4.0],
        cells: [16, 16],
        amplitude: 1.0,
        frequency: 2.0,
    };
    let glb = |seed| concordia_gen::glb::write_glb(&build(&req, seed).unwrap()).unwrap();
    assert_eq!(glb(3), glb(3));
    assert_ne!(glb(3), glb(4));
    let bytes = glb(3);
    assert_eq!(&bytes[0..4], b"glTF", "a GLB header");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        2,
        "glTF 2"
    );
    assert_eq!(
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize,
        bytes.len()
    );
}

/// Requests are validated: a radius must be positive, segments at least 3, a profile at least
/// 3 points, an SDF at least one shape and a cell size at most a tenth of its extent.
#[test]
fn bad_requests_are_refused_naming_the_field() {
    let bad = [
        (
            MeshRequest::Sphere {
                radius: 0.0,
                segments: 16,
                rings: 8,
            },
            "radius",
        ),
        (
            MeshRequest::Cylinder {
                radius: 1.0,
                height: 1.0,
                segments: 2,
            },
            "segments",
        ),
        (
            MeshRequest::Extrude {
                profile: vec![[0.0, 0.0], [1.0, 0.0]],
                depth: 1.0,
            },
            "profile",
        ),
        (
            MeshRequest::Box {
                size: [1.0, -1.0, 1.0],
            },
            "size",
        ),
        (
            MeshRequest::Sdf {
                shapes: vec![],
                smooth: 0.0,
                cell: 0.1,
            },
            "shapes",
        ),
        (
            MeshRequest::Sdf {
                shapes: vec![SdfShape::Sphere {
                    center: [0.0; 3],
                    radius: 1.0,
                }],
                smooth: 0.0,
                cell: 1.0,
            },
            "cell",
        ),
    ];
    for (req, field) in bad {
        let err = build(&req, 0).unwrap_err().to_string();
        assert!(err.contains(field), "{req:?}: {err}");
    }
}
