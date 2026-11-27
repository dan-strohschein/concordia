//! Refit: a tree whose primitives moved finds what a rebuilt one finds (M4 4.8a).

use concordia_bvh::{Accel, Bvh, BvhError, Instance, Ray, Triangle};
use concordia_core::math::{Mat4, Vec3};
use proptest::prelude::*;

fn v3(r: f32) -> impl Strategy<Value = Vec3> {
    (-r..r, -r..r, -r..r).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

fn soup() -> impl Strategy<Value = Vec<Triangle>> {
    proptest::collection::vec(
        (v3(10.0), v3(10.0), v3(10.0)).prop_map(|(a, b, c)| Triangle { v: [a, b, c] }),
        1..120,
    )
}

fn rays() -> impl Strategy<Value = Vec<Ray>> {
    proptest::collection::vec(
        (v3(12.0), v3(1.0))
            .prop_filter("a direction", |(_, d)| d.length() > 1e-2)
            .prop_map(|(origin, dir)| Ray {
                origin,
                dir,
                t_min: 0.0,
                t_max: f32::INFINITY,
            }),
        1..16,
    )
}

/// The nearest distance over every triangle.
fn brute_force(triangles: &[Triangle], ray: &Ray) -> Option<f32> {
    triangles
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.intersect(ray, i as u32))
        .map(|h| h.t)
        .min_by(f32::total_cmp)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn refit_matches_rebuild_hits(
        triangles in soup(),
        moves in proptest::collection::vec(v3(6.0), 120),
        rays in rays(),
    ) {
        let moved: Vec<Triangle> = triangles
            .iter()
            .zip(&moves)
            .map(|(t, d)| Triangle { v: t.v.map(|v| v + *d) })
            .collect();
        let mut bvh = Bvh::build(&triangles);
        bvh.refit(&moved).unwrap();
        for ray in &rays {
            let want = brute_force(&moved, ray);
            prop_assert_eq!(bvh.intersect(ray).map(|h| h.t), want);
            prop_assert_eq!(bvh.occluded(ray), want.is_some());
        }
    }

    #[test]
    fn moved_instances_hit_like_a_rebuilt_accel(
        mesh in soup(),
        before in proptest::collection::vec(v3(15.0), 1..10),
        after in proptest::collection::vec(v3(15.0), 10),
        rays in rays(),
    ) {
        let place = |p: &Vec3| Instance { mesh: 0, world: Mat4::from_translation(*p) };
        let mut accel = Accel::build(vec![Bvh::build(&mesh)], before.iter().map(place).collect());
        let moves: Vec<(u32, Mat4)> = (0..before.len() as u32)
            .map(|i| (i, Mat4::from_translation(after[i as usize])))
            .collect();
        accel.set_transforms(&moves).unwrap();
        let rebuilt = Accel::build(
            vec![Bvh::build(&mesh)],
            after[..before.len()].iter().map(place).collect(),
        );
        for ray in &rays {
            prop_assert_eq!(
                accel.intersect(ray).map(|h| h.t),
                rebuilt.intersect(ray).map(|h| h.t)
            );
            prop_assert_eq!(accel.occluded(ray), rebuilt.occluded(ray));
        }
    }
}

#[test]
fn refit_rejects_a_different_triangle_count() {
    let t = Triangle {
        v: [Vec3::ZERO, Vec3::X, Vec3::Y],
    };
    let mut bvh = Bvh::build(&[t, t]);
    assert_eq!(
        bvh.refit(&[t]),
        Err(BvhError::CountMismatch {
            expected: 2,
            got: 1
        })
    );
}

#[test]
fn set_transforms_rejects_a_missing_instance() {
    let t = Triangle {
        v: [Vec3::ZERO, Vec3::X, Vec3::Y],
    };
    let mut accel = Accel::build(
        vec![Bvh::build(&[t])],
        vec![Instance {
            mesh: 0,
            world: Mat4::IDENTITY,
        }],
    );
    assert_eq!(
        accel.set_transforms(&[(1, Mat4::IDENTITY)]),
        Err(BvhError::NoInstance { index: 1, count: 1 })
    );
    let singular = Mat4::from_scale(Vec3::ZERO);
    assert_eq!(
        accel.set_transforms(&[(0, singular)]),
        Err(BvhError::Singular { index: 0 })
    );
}
