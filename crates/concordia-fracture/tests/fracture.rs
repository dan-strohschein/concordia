//! Fracture (M8 8.1a): a closed mesh cut into pieces by a seeded pattern. Sources are extruded
//! polygons: a 2 × 1 × 1 box (volume 2) and an L-shaped prism (volume 3), which is concave.

use concordia_fracture::{Fracture, FractureError, Mesh, Pattern, Slot, fracture};
use proptest::prelude::*;

/// The prism of a counter-clockwise polygon (fan-triangulated from vertex 0) from z = 0 to `h`.
fn extrude(polygon: &[[f32; 2]], h: f32) -> Mesh {
    let n = polygon.len() as u32;
    let mut positions: Vec<[f32; 3]> = polygon.iter().map(|p| [p[0], p[1], 0.0]).collect();
    positions.extend(polygon.iter().map(|p| [p[0], p[1], h]));
    let mut triangles = Vec::new();
    for i in 1..n - 1 {
        triangles.push([0, i + 1, i]); // bottom, facing -z
        triangles.push([n, n + i, n + i + 1]); // top, facing +z
    }
    for i in 0..n {
        let j = (i + 1) % n;
        triangles.push([i, j, j + n]);
        triangles.push([i, j + n, i + n]);
    }
    Mesh {
        positions,
        triangles,
    }
}

fn slab() -> Mesh {
    extrude(&[[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]], 1.0)
}

fn ell() -> Mesh {
    let l = [
        [0.0, 0.0],
        [2.0, 0.0],
        [2.0, 1.0],
        [1.0, 1.0],
        [1.0, 2.0],
        [0.0, 2.0],
    ];
    extrude(&l, 1.0)
}

fn patterns() -> Vec<Pattern> {
    vec![
        Pattern::Uniform { cells: 12 },
        Pattern::Clustered {
            clusters: 3,
            per_cluster: 4,
            spread: 0.2,
        },
        Pattern::Radial {
            impact: [0.5, 0.5, 1.0],
            cells: 10,
            falloff: 2.0,
        },
        Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 4,
        },
        Pattern::Bricks {
            size: [0.5, 0.25, 1.0],
        },
    ]
}

/// Every byte of the result, in order.
fn bytes(f: &Fracture) -> Vec<u8> {
    let mut out = Vec::new();
    for p in &f.pieces {
        out.extend(p.cell.to_le_bytes());
        out.extend(p.volume.to_le_bytes());
        for v in p.positions.iter().flatten() {
            out.extend(v.to_le_bytes());
        }
        for t in p.triangles.iter().flatten() {
            out.extend(t.to_le_bytes());
        }
        out.extend(p.materials.iter().map(|s| *s as u8));
    }
    out
}

fn total(f: &Fracture) -> f64 {
    f.pieces.iter().map(|p| p.volume).sum()
}

/// Generalised winding number of `q` for a closed triangle mesh: 1 inside, 0 outside.
fn winding(positions: &[[f32; 3]], triangles: &[[u32; 3]], q: [f64; 3]) -> f64 {
    let sub = |v: [f32; 3]| [v[0] as f64 - q[0], v[1] as f64 - q[1], v[2] as f64 - q[2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let len = |a: [f64; 3]| dot(a, a).sqrt();
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let mut sum = 0.0;
    for t in triangles {
        let [a, b, c] = t.map(|i| sub(positions[i as usize]));
        let (la, lb, lc) = (len(a), len(b), len(c));
        let num = dot(a, cross(b, c));
        let den = la * lb * lc + dot(a, b) * lc + dot(b, c) * la + dot(c, a) * lb;
        sum += 2.0 * num.atan2(den);
    }
    sum / (4.0 * std::f64::consts::PI)
}

#[test]
fn same_seed_same_pieces_byte_for_byte() {
    for pattern in patterns() {
        let a = fracture(&ell(), &pattern, 7).unwrap();
        let b = fracture(&ell(), &pattern, 7).unwrap();
        assert!(!a.pieces.is_empty(), "{pattern:?}");
        assert_eq!(bytes(&a), bytes(&b), "{pattern:?}");
    }
    let seeded = |seed| bytes(&fracture(&ell(), &Pattern::Uniform { cells: 12 }, seed).unwrap());
    assert_ne!(seeded(7), seeded(8), "the seed matters");
}

#[test]
fn pieces_partition_the_volume() {
    for (mesh, volume) in [(slab(), 2.0), (ell(), 3.0)] {
        for pattern in patterns() {
            let f = fracture(&mesh, &pattern, 3).unwrap();
            assert!(
                (total(&f) - volume).abs() < 1e-4,
                "{pattern:?}: {} of {volume}",
                total(&f)
            );
            // No overlaps: sample a grid; every point inside the source is in exactly one piece.
            for i in 0..8 {
                for j in 0..8 {
                    for k in 0..4 {
                        let q = [
                            0.13 + i as f64 * 0.25,
                            0.11 + j as f64 * 0.25,
                            0.12 + k as f64 * 0.25,
                        ];
                        let inside = winding(&mesh.positions, &mesh.triangles, q).round();
                        let holders: f64 = f
                            .pieces
                            .iter()
                            .map(|p| winding(&p.positions, &p.triangles, q).round())
                            .sum();
                        assert_eq!(holders, inside, "{pattern:?} at {q:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn every_piece_is_closed_and_manifold() {
    for pattern in patterns() {
        let f = fracture(&ell(), &pattern, 5).unwrap();
        for p in &f.pieces {
            assert!(p.volume > 0.0, "{pattern:?}: an empty piece");
            assert_eq!(p.materials.len(), p.triangles.len());
            // Every directed edge appears once, and so does its reverse.
            let mut edges = std::collections::BTreeMap::new();
            for t in &p.triangles {
                for e in 0..3 {
                    *edges.entry((t[e], t[(e + 1) % 3])).or_insert(0) += 1;
                }
            }
            for (&(a, b), &n) in &edges {
                assert_eq!(n, 1, "{pattern:?}: edge {a}→{b} used {n} times");
                assert_eq!(edges.get(&(b, a)), Some(&1), "{pattern:?}: {a}→{b} is open");
            }
        }
    }
}

#[test]
fn interior_faces_use_the_interior_material() {
    // Two slices of the slab along x: the cut is the plane x = 1.
    let f = fracture(
        &slab(),
        &Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 2,
        },
        0,
    )
    .unwrap();
    assert_eq!(f.pieces.len(), 2);
    for p in &f.pieces {
        assert!((p.volume - 1.0).abs() < 1e-5, "{}", p.volume);
        let mut cut = 0;
        for (t, slot) in p.triangles.iter().zip(&p.materials) {
            let on_cut = t
                .iter()
                .all(|&v| (p.positions[v as usize][0] - 1.0).abs() < 1e-5);
            assert_eq!(*slot == Slot::Interior, on_cut, "{t:?} is {slot:?}");
            cut += usize::from(on_cut);
        }
        assert!(cut >= 2, "the cut face is there");
    }
}

#[test]
fn slices_and_bricks_cut_on_their_grid() {
    let slices = fracture(
        &slab(),
        &Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 4,
        },
        0,
    )
    .unwrap();
    let volumes: Vec<f64> = slices.pieces.iter().map(|p| p.volume).collect();
    assert_eq!(volumes.len(), 4);
    assert!(
        volumes.iter().all(|v| (v - 0.5).abs() < 1e-5),
        "{volumes:?}"
    );
    // Bricks of 1 × 0.5 × 1 on the slab: two rows; the upper is offset by half a brick, so it
    // has two half bricks at its ends.
    let bricks = fracture(
        &slab(),
        &Pattern::Bricks {
            size: [1.0, 0.5, 1.0],
        },
        0,
    )
    .unwrap();
    let mut volumes: Vec<f64> = bricks.pieces.iter().map(|p| p.volume).collect();
    volumes.sort_by(f64::total_cmp);
    let want = [0.25, 0.25, 0.5, 0.5, 0.5];
    assert_eq!(volumes.len(), want.len(), "{volumes:?}");
    for (v, w) in volumes.iter().zip(want) {
        assert!((v - w).abs() < 1e-5, "{volumes:?}");
    }
}

#[test]
fn radial_pieces_are_smaller_near_the_impact() {
    let impact = [0.2, 0.2, 0.5];
    let f = fracture(
        &slab(),
        &Pattern::Radial {
            impact,
            cells: 24,
            falloff: 3.0,
        },
        11,
    )
    .unwrap();
    let centre = |p: &concordia_fracture::Piece| {
        let n = p.positions.len() as f32;
        let s = p.positions.iter().fold([0.0f32; 3], |a, v| {
            [a[0] + v[0] / n, a[1] + v[1] / n, a[2] + v[2] / n]
        });
        ((s[0] - impact[0]).powi(2) + (s[1] - impact[1]).powi(2) + (s[2] - impact[2]).powi(2))
            .sqrt()
    };
    let mut by_distance: Vec<_> = f.pieces.iter().map(|p| (centre(p), p.volume)).collect();
    by_distance.sort_by(|a, b| a.0.total_cmp(&b.0));
    let half = by_distance.len() / 2;
    let near: f64 = by_distance[..half].iter().map(|x| x.1).sum::<f64>() / half as f64;
    let far: f64 =
        by_distance[half..].iter().map(|x| x.1).sum::<f64>() / (by_distance.len() - half) as f64;
    assert!(near < far, "near {near}, far {far}");
}

#[test]
fn degenerate_inputs_are_errors_not_panics() {
    let uniform = Pattern::Uniform { cells: 4 };
    // An open mesh: the slab without its last triangle.
    let mut open = slab();
    open.triangles.pop();
    assert_eq!(fracture(&open, &uniform, 0), Err(FractureError::NotClosed));
    let mut nan = slab();
    nan.positions[0][1] = f32::NAN;
    assert!(matches!(
        fracture(&nan, &uniform, 0),
        Err(FractureError::InvalidMesh(_))
    ));
    let mut out_of_range = slab();
    out_of_range.triangles[0][0] = 99;
    assert!(matches!(
        fracture(&out_of_range, &uniform, 0),
        Err(FractureError::InvalidMesh(_))
    ));
    assert!(matches!(
        fracture(&Mesh::default(), &uniform, 0),
        Err(FractureError::InvalidMesh(_))
    ));
    let bad_patterns = [
        Pattern::Uniform { cells: 0 },
        Pattern::Uniform { cells: 100_000 },
        Pattern::Clustered {
            clusters: 2,
            per_cluster: 0,
            spread: 0.1,
        },
        Pattern::Clustered {
            clusters: 2,
            per_cluster: 2,
            spread: f32::NAN,
        },
        Pattern::Radial {
            impact: [f32::INFINITY, 0.0, 0.0],
            cells: 4,
            falloff: 1.0,
        },
        Pattern::Radial {
            impact: [0.0; 3],
            cells: 4,
            falloff: 0.0,
        },
        Pattern::Slices {
            normal: [0.0; 3],
            count: 2,
        },
        Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 0,
        },
        Pattern::Bricks {
            size: [0.0, 1.0, 1.0],
        },
        Pattern::Bricks {
            size: [1e-4, 1e-4, 1e-4],
        },
    ];
    for p in bad_patterns {
        assert!(
            matches!(
                fracture(&slab(), &p, 0),
                Err(FractureError::InvalidPattern(_))
            ),
            "{p:?}"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(12))]

    #[test]
    fn random_seeds_and_cell_counts_keep_the_volume(seed in any::<u64>(), cells in 1u32..20) {
        let f = fracture(&ell(), &Pattern::Uniform { cells }, seed).unwrap();
        prop_assert!((total(&f) - 3.0).abs() < 1e-4, "{} pieces: {}", f.pieces.len(), total(&f));
        prop_assert!(f.pieces.len() <= cells as usize);
    }
}
