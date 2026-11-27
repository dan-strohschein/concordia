//! `safe::simplify` (M4 4.8a): fewer triangles within the asked error, and refusals.

use meshopt_sys::{MeshoptError, safe};
use proptest::prelude::*;

/// An n × n grid of quads over `height(x, z)`, in a unit square.
fn grid(n: u32, height: impl Fn(f32, f32) -> f32) -> (Vec<u32>, Vec<[f32; 3]>) {
    let side = n + 1;
    let positions = (0..side * side)
        .map(|i| {
            let (x, z) = ((i % side) as f32 / n as f32, (i / side) as f32 / n as f32);
            [x, height(x, z), z]
        })
        .collect();
    let mut indices = Vec::new();
    for row in 0..n {
        for col in 0..n {
            let a = row * side + col;
            indices.extend([a, a + side, a + 1, a + 1, a + side, a + side + 1]);
        }
    }
    (indices, positions)
}

#[test]
fn a_flat_grid_collapses_to_a_few_triangles() {
    let (indices, positions) = grid(32, |_, _| 0.0);
    let out = safe::simplify(&indices, &positions, 0, 0.01).unwrap();
    assert!(
        out.indices.len() / 3 <= 8,
        "{} triangles",
        out.indices.len() / 3
    );
    assert!(out.error <= 0.01, "error {}", out.error);
}

#[test]
fn a_curved_surface_keeps_its_shape_within_the_error() {
    let (indices, positions) = grid(32, |x, z| 0.2 * (x * 6.0).sin() * (z * 6.0).cos());
    let out = safe::simplify(&indices, &positions, 0, 0.01).unwrap();
    let (before, after) = (indices.len() / 3, out.indices.len() / 3);
    assert!(after < before / 4, "{before} → {after} triangles");
    assert!(after > 8, "the bumps survive: {after}");
    assert!(out.error <= 0.01, "error {}", out.error);
}

#[test]
fn simplify_refuses_bad_input() {
    let (indices, positions) = grid(2, |_, _| 0.0);
    assert_eq!(
        safe::simplify(&indices[..4], &positions, 0, 0.01),
        Err(MeshoptError::IndexCount(4))
    );
    assert_eq!(
        safe::simplify(&[0, 1, 99], &positions, 0, 0.01),
        Err(MeshoptError::IndexRange {
            index: 99,
            vertices: positions.len()
        })
    );
    let mut bad = positions.clone();
    bad[1][0] = f32::NAN;
    assert_eq!(
        safe::simplify(&indices, &bad, 0, 0.01),
        Err(MeshoptError::NonFinitePosition(1))
    );
    // meshoptimizer scales by every vertex's extent, so an unreferenced one counts too.
    let mut spare = positions.clone();
    spare.push([f32::INFINITY, 0.0, 0.0]);
    assert_eq!(
        safe::simplify(&indices, &spare, 0, 0.01),
        Err(MeshoptError::NonFinitePosition(positions.len()))
    );
    assert_eq!(
        safe::simplify(&indices, &positions, 0, 1.5),
        Err(MeshoptError::TargetError(1.5))
    );
    assert!(matches!(
        safe::simplify(&indices, &positions, 0, f32::NAN),
        Err(MeshoptError::TargetError(v)) if v.is_nan()
    ));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn simplified_indices_are_a_valid_smaller_triangle_list(
        n in 1u32..12,
        amp in 0.0f32..0.5,
        target in 0u32..200,
        error in 0.0f32..0.2,
    ) {
        let (indices, positions) = grid(n, |x, z| amp * (x * 5.0 + z * 3.0).sin());
        let out = safe::simplify(&indices, &positions, target as usize * 3, error).unwrap();
        prop_assert!(out.indices.len().is_multiple_of(3));
        prop_assert!(out.indices.len() <= indices.len());
        prop_assert!(out.indices.iter().all(|&i| (i as usize) < positions.len()));
        prop_assert!(out.error.is_finite() && out.error >= 0.0);
    }
}
