//! The BVH finds exactly what testing every triangle finds (M4 4.1b2).

use concordia_core::math::Vec3;
use concordia_pathtrace::{Bvh, Ray, Triangle};
use proptest::prelude::*;

fn brute_force(triangles: &[Triangle], ray: &Ray) -> Option<(f32, u32)> {
    triangles
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.intersect(ray, i as u32))
        .map(|h| (h.t, h.prim))
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
}

fn v3() -> impl Strategy<Value = Vec3> {
    (-10.0f32..10.0, -10.0f32..10.0, -10.0f32..10.0).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

fn soup() -> impl Strategy<Value = Vec<Triangle>> {
    proptest::collection::vec(
        (v3(), v3(), v3()).prop_map(|(a, b, c)| Triangle { v: [a, b, c] }),
        0..120,
    )
}

fn ray() -> impl Strategy<Value = Ray> {
    (v3(), v3())
        .prop_filter("a direction", |(_, d)| d.length() > 1e-3)
        .prop_map(|(origin, dir)| Ray {
            origin,
            dir,
            t_min: 0.0,
            t_max: f32::INFINITY,
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn pathtrace_bvh_traversal_matches_brute_force(triangles in soup(), rays in proptest::collection::vec(ray(), 1..16)) {
        let bvh = Bvh::build(&triangles);
        for ray in &rays {
            let want = brute_force(&triangles, ray);
            let got = bvh.intersect(ray).map(|h| (h.t, h.prim));
            match (want, got) {
                (None, None) => {}
                (Some((wt, wp)), Some((gt, gp))) => {
                    prop_assert_eq!(wt, gt, "same distance");
                    // Equal distances may hit either triangle; otherwise the same one.
                    if wp != gp {
                        let other = triangles[gp as usize].intersect(ray, gp).map(|h| h.t);
                        prop_assert_eq!(other, Some(wt), "a tie, not a different surface");
                    }
                }
                _ => prop_assert!(false, "bvh {:?} vs brute force {:?}", got, want),
            }
            prop_assert_eq!(bvh.occluded(ray), want.is_some(), "occlusion agrees");
        }
    }
}

#[test]
fn a_ray_hits_a_triangle_in_front_and_not_one_behind() {
    let t = Triangle {
        v: [
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
    };
    let toward = Ray {
        origin: Vec3::new(0.0, 0.0, 2.0),
        dir: Vec3::new(0.0, 0.0, -1.0),
        t_min: 0.0,
        t_max: f32::INFINITY,
    };
    let hit = t.intersect(&toward, 7).expect("a hit");
    assert!((hit.t - 2.0).abs() < 1e-5 && hit.prim == 7, "{hit:?}");
    assert!((1.0 - hit.u - hit.w) >= 0.0 && hit.u >= 0.0 && hit.w >= 0.0);
    let away = Ray {
        dir: Vec3::new(0.0, 0.0, 1.0),
        ..toward
    };
    assert_eq!(t.intersect(&away, 7), None, "behind the origin");
    let short = Ray {
        t_max: 1.5,
        ..toward
    };
    assert_eq!(t.intersect(&short, 7), None, "beyond t_max");
}

#[test]
fn an_empty_bvh_hits_nothing() {
    let ray = Ray {
        origin: Vec3::ZERO,
        dir: Vec3::X,
        t_min: 0.0,
        t_max: f32::INFINITY,
    };
    let bvh = Bvh::build(&[]);
    assert_eq!(bvh.intersect(&ray), None);
    assert!(!bvh.occluded(&ray));
}

/// A grid of separate triangles builds a real tree: small leaves covering every triangle once,
/// each node's box holding its children's.
#[test]
fn the_bvh_splits_into_small_leaves_covering_every_triangle_once() {
    let triangles: Vec<Triangle> = (0..1000)
        .map(|i| {
            let o = Vec3::new((i % 10) as f32, ((i / 10) % 10) as f32, (i / 100) as f32) * 3.0;
            Triangle {
                v: [o, o + Vec3::X, o + Vec3::Y],
            }
        })
        .collect();
    let bvh = Bvh::build(&triangles);
    let leaves: Vec<_> = bvh.nodes().iter().filter(|n| n.count > 0).collect();
    assert!(leaves.len() >= 250, "{} leaves", leaves.len());
    assert!(leaves.iter().all(|n| n.count <= 4));
    let mut seen: Vec<u32> = leaves
        .iter()
        .flat_map(|n| bvh.order()[n.first as usize..(n.first + n.count) as usize].to_vec())
        .collect();
    seen.sort_unstable();
    assert_eq!(seen, (0..1000).collect::<Vec<u32>>());
    for (i, n) in bvh.nodes().iter().enumerate().filter(|(_, n)| n.count == 0) {
        for child in [&bvh.nodes()[i + 1], &bvh.nodes()[n.first as usize]] {
            assert!(child.min.cmpge(n.min).all() && child.max.cmple(n.max).all());
        }
    }
}
