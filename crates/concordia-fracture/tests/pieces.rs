//! Per-piece data for play (M8 8.1b): a collision hull and interior UVs. Sources: a 2 × 1 × 1
//! box and an L-shaped prism (concave), as in `fracture.rs`.

use concordia_core::math::Vec3;
use concordia_fracture::{FractureError, Mesh, Pattern, Slot, fracture};
use concordia_physics::{ColliderShape, hull_triangles};

fn extrude(polygon: &[[f32; 2]], h: f32) -> Mesh {
    let n = polygon.len() as u32;
    let mut positions: Vec<[f32; 3]> = polygon.iter().map(|p| [p[0], p[1], 0.0]).collect();
    positions.extend(polygon.iter().map(|p| [p[0], p[1], h]));
    let mut triangles = Vec::new();
    for i in 1..n - 1 {
        triangles.push([0, i + 1, i]);
        triangles.push([n, n + i, n + i + 1]);
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

#[test]
fn hull_contains_its_piece() {
    // Slices of the L along a diagonal leave concave pieces; their hulls must still hold them.
    for pattern in [
        Pattern::Uniform { cells: 8 },
        Pattern::Slices {
            normal: [1.0, 1.0, 0.0],
            count: 3,
        },
    ] {
        let f = fracture(&ell(), &pattern, 2).unwrap();
        for p in &f.pieces {
            let ColliderShape::ConvexHull { points } = p.collider().unwrap() else {
                panic!("a piece's collider is a convex hull");
            };
            let triangles = hull_triangles(&points).unwrap();
            // The collider is in piece space: relative to the piece's centre of mass.
            let centre = p.centre_of_mass();
            for v in &p.positions {
                let v = Vec3::from_array(*v) - centre;
                for [a, b, c] in &triangles {
                    let n = (*b - *a).cross(*c - *a).normalize();
                    // Jolt's hull tolerance is 1 mm.
                    assert!(
                        n.dot(v - *a) <= 2e-3,
                        "{pattern:?}: {v} is {} outside",
                        n.dot(v - *a)
                    );
                }
            }
        }
    }
}

#[test]
fn a_piece_collider_is_in_piece_space_around_its_centre_of_mass() {
    let f = fracture(
        &slab(),
        &Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 2,
        },
        0,
    )
    .unwrap();
    // The x ≥ 1 half: its centre of mass is (1.5, 0.5, 0.5).
    let right = &f.pieces[1];
    assert!((right.centre_of_mass() - Vec3::new(1.5, 0.5, 0.5)).length() < 1e-5);
    let ColliderShape::ConvexHull { points } = right.collider().unwrap() else {
        panic!("a convex hull");
    };
    let (lo, hi) = points.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    assert!((lo - Vec3::splat(-0.5)).length() < 1e-5, "{lo}");
    assert!((hi - Vec3::splat(0.5)).length() < 1e-5, "{hi}");
}

#[test]
fn interior_uvs_match_across_a_cut() {
    let f = fracture(
        &slab(),
        &Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 2,
        },
        0,
    )
    .unwrap();
    // Each side's UV at each cut-face corner, keyed by its position.
    let uv_at = |piece: usize| {
        let p = &f.pieces[piece];
        let uvs = p.interior_uvs(1.0).unwrap();
        let mut out = Vec::new();
        for ((t, slot), uv) in p.triangles.iter().zip(&p.materials).zip(&uvs) {
            match slot {
                Slot::Interior => {
                    for k in 0..3 {
                        out.push((p.positions[t[k] as usize], uv[k]));
                    }
                }
                Slot::Surface => assert_eq!(*uv, [[0.0; 2]; 3], "surface faces get none"),
            }
        }
        out
    };
    let (left, right) = (uv_at(0), uv_at(1));
    assert!(!left.is_empty() && !right.is_empty());
    for (pos, uv) in &left {
        for (other, theirs) in &right {
            if pos == other {
                let close = (uv[0] - theirs[0]).abs() < 1e-5 && (uv[1] - theirs[1]).abs() < 1e-5;
                assert!(
                    close,
                    "both sides of the cut agree at {pos:?}: {uv:?} vs {theirs:?}"
                );
            }
        }
    }
    // A unit-square cut face at scale 1 spans one unit of UV on each axis.
    let span = |axis: usize| {
        let vals: Vec<f32> = left.iter().map(|(_, uv)| uv[axis]).collect();
        vals.iter().copied().fold(f32::MIN, f32::max)
            - vals.iter().copied().fold(f32::MAX, f32::min)
    };
    assert!((span(0) - 1.0).abs() < 1e-5 && (span(1) - 1.0).abs() < 1e-5);
}

#[test]
fn interior_uvs_follow_the_scale() {
    let f = fracture(&ell(), &Pattern::Uniform { cells: 6 }, 9).unwrap();
    for p in &f.pieces {
        let (one, half) = (p.interior_uvs(1.0).unwrap(), p.interior_uvs(0.5).unwrap());
        assert_eq!(one.len(), p.triangles.len());
        for (a, b) in one.iter().zip(&half) {
            for k in 0..3 {
                for c in 0..2 {
                    assert!((a[k][c] * 2.0 - b[k][c]).abs() < 1e-4, "{a:?} vs {b:?}");
                }
            }
        }
    }
    // A scale that isn't a positive number is an error.
    let p = &f.pieces[0];
    for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(
            matches!(p.interior_uvs(bad), Err(FractureError::InvalidParameter(_))),
            "{bad}"
        );
    }
}
