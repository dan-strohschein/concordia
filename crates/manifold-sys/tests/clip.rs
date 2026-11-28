//! manifold-sys: clipping a closed mesh by a convex cell (M8 8.1a, ADR-0180), the operation
//! fracture is made of. Values by hand: a unit cube centred on the origin (volume 1).

use manifold_sys::safe::{Mesh, Plane, clip_convex};

/// A unit cube centred on the origin: 8 corners, 12 outward triangles.
fn cube() -> Mesh {
    let mut positions = Vec::new();
    for i in 0..8 {
        let s = |bit: i32| if i & bit == 0 { -0.5 } else { 0.5 };
        positions.push([s(1), s(2), s(4)]);
    }
    // Corner index = x + 2y + 4z (bit set: +0.5).
    let triangles = vec![
        [0, 4, 6],
        [0, 6, 2], // -x
        [1, 3, 7],
        [1, 7, 5], // +x
        [0, 1, 5],
        [0, 5, 4], // -y
        [2, 6, 7],
        [2, 7, 3], // +y
        [0, 2, 3],
        [0, 3, 1], // -z
        [4, 5, 7],
        [4, 7, 6], // +z
    ];
    Mesh {
        positions,
        triangles,
    }
}

fn plane(normal: [f32; 3], offset: f32) -> Plane {
    Plane { normal, offset }
}

#[test]
fn a_half_space_keeps_half_the_cube_and_marks_the_cut_as_interior() {
    // Keep n·x ≤ d with n = +x, d = 0: the x ≤ 0 half.
    let piece = clip_convex(&cube(), &[plane([1.0, 0.0, 0.0], 0.0)]).unwrap();
    assert!((piece.volume - 0.5).abs() < 1e-5, "{}", piece.volume);
    assert!(
        piece.positions.iter().all(|p| p[0] <= 1e-6),
        "nothing past the plane"
    );
    let interior: Vec<_> = (0..piece.triangles.len())
        .filter(|&t| piece.interior[t])
        .collect();
    assert!(!interior.is_empty(), "the cut face is interior");
    for t in interior {
        assert!(
            piece.triangles[t]
                .iter()
                .all(|&v| piece.positions[v as usize][0].abs() < 1e-6),
            "interior faces lie on the cut"
        );
    }
}

#[test]
fn a_cell_holding_the_whole_mesh_returns_it_unchanged_and_a_cell_beside_it_returns_nothing() {
    let all = clip_convex(&cube(), &[plane([1.0, 0.0, 0.0], 5.0)]).unwrap();
    assert!((all.volume - 1.0).abs() < 1e-5);
    assert!(all.interior.iter().all(|i| !i), "no cut, no interior faces");
    let none = clip_convex(&cube(), &[plane([1.0, 0.0, 0.0], -5.0)]).unwrap();
    assert_eq!(none.volume, 0.0);
    assert!(none.triangles.is_empty());
}

#[test]
fn a_corner_cell_is_an_eighth_and_clipping_is_deterministic() {
    let cell = [
        plane([1.0, 0.0, 0.0], 0.0),
        plane([0.0, 1.0, 0.0], 0.0),
        plane([0.0, 0.0, 1.0], 0.0),
    ];
    let piece = clip_convex(&cube(), &cell).unwrap();
    assert!((piece.volume - 0.125).abs() < 1e-5, "{}", piece.volume);
    assert_eq!(
        piece,
        clip_convex(&cube(), &cell).unwrap(),
        "same input, same piece"
    );
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    let mut open = cube();
    open.triangles.pop();
    assert!(clip_convex(&open, &[]).is_err(), "an open mesh");
    let mut nan = cube();
    nan.positions[3][1] = f32::NAN;
    assert!(clip_convex(&nan, &[]).is_err(), "NaN");
    let mut out_of_range = cube();
    out_of_range.triangles[0][2] = 99;
    assert!(
        clip_convex(&out_of_range, &[]).is_err(),
        "an index out of range"
    );
    assert!(
        clip_convex(&cube(), &[plane([0.0, 0.0, 0.0], 0.0)]).is_err(),
        "a zero normal"
    );
}
