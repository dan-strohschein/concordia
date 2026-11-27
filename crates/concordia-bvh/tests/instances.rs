//! Instancing: the two-level structure finds what testing every placed triangle finds.

use concordia_bvh::{Accel, Bvh, Instance, Ray, Triangle};
use concordia_core::math::{Mat4, Quat, Vec3};
use proptest::prelude::*;

fn v3(r: f32) -> impl Strategy<Value = Vec3> {
    (-r..r, -r..r, -r..r).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

fn mesh() -> impl Strategy<Value = Vec<Triangle>> {
    proptest::collection::vec(
        (v3(2.0), v3(2.0), v3(2.0)).prop_map(|(a, b, c)| Triangle { v: [a, b, c] }),
        1..30,
    )
}

fn placement() -> impl Strategy<Value = Mat4> {
    (v3(8.0), v3(1.0), 0.2f32..3.0, 0.2f32..3.0, 0.2f32..3.0).prop_map(|(t, axis, sx, sy, sz)| {
        let rotation = if axis.length() > 1e-3 {
            Quat::from_axis_angle(axis.normalize(), axis.length() * 3.0)
        } else {
            Quat::IDENTITY
        };
        Mat4::from_scale_rotation_translation(Vec3::new(sx, sy, sz), rotation, t)
    })
}

/// The nearest hit over every instance's triangles moved into the world: (t, instance, prim,
/// distance from the nearest edge in barycentrics).
fn brute_force(
    meshes: &[Vec<Triangle>],
    instances: &[Instance],
    ray: &Ray,
) -> Option<(f32, u32, u32, f32)> {
    instances
        .iter()
        .enumerate()
        .flat_map(|(i, inst)| {
            meshes[inst.mesh as usize]
                .iter()
                .enumerate()
                .filter_map(move |(p, t)| {
                    let world = Triangle {
                        v: t.v.map(|v| inst.world.transform_point3(v)),
                    };
                    world
                        .intersect(ray, p as u32)
                        .map(|h| (h.t, i as u32, h.prim, edge(h.u, h.w)))
                })
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// How far barycentrics (u, w) are from the triangle's nearest edge.
fn edge(u: f32, w: f32) -> f32 {
    u.min(w).min(1.0 - u - w)
}

/// World- and object-space tests round differently, so they may disagree only this close to an
/// edge (Möller–Trumbore is not watertight across spaces; a closed mesh's neighbour covers it).
const EDGE: f32 = 1e-4;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn instanced_scene_matches_its_placed_triangles(
        meshes in proptest::collection::vec(mesh(), 1..4),
        placements in proptest::collection::vec((0u32..4, placement()), 0..12),
        rays in proptest::collection::vec((v3(12.0), v3(1.0)), 1..12),
    ) {
        let instances: Vec<Instance> = placements
            .iter()
            .map(|(m, world)| Instance { mesh: m % meshes.len() as u32, world: *world })
            .collect();
        let accel = Accel::build(meshes.iter().map(|m| Bvh::build(m)).collect(), instances.clone());
        for (origin, dir) in rays {
            prop_assume!(dir.length() > 1e-2);
            let ray = Ray { origin, dir, t_min: 0.0, t_max: f32::INFINITY };
            let want = brute_force(&meshes, &instances, &ray);
            let got = accel.intersect(&ray);
            match (want, got) {
                (None, None) => {}
                (Some((wt, _, _, e)), Some(h)) => {
                    // Rays are moved into object space, so distances agree to rounding; a
                    // farther hit is allowed only when the nearer one sits on an edge.
                    let same = (wt - h.t).abs() <= 1e-3 * wt.max(1.0);
                    prop_assert!(same || (h.t > wt && e < EDGE), "t {} vs {}", wt, h.t);
                }
                (Some((_, _, _, e)), None) => prop_assert!(e < EDGE, "missed {:?}", want),
                (None, Some(h)) => prop_assert!(edge(h.u, h.w) < EDGE, "extra {:?}", h),
            }
            prop_assert_eq!(accel.occluded(&ray), got.is_some(), "occlusion agrees with intersect");
        }
    }
}

#[test]
fn a_mesh_placed_twice_is_hit_in_both_places() {
    let quad = vec![
        Triangle {
            v: [
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
            ],
        },
        Triangle {
            v: [
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(-1.0, 1.0, 0.0),
            ],
        },
    ];
    let at = |x: f32| Instance {
        mesh: 0,
        world: Mat4::from_translation(Vec3::new(x, 0.0, -5.0)),
    };
    let accel = Accel::build(vec![Bvh::build(&quad)], vec![at(-3.0), at(3.0)]);
    let ray = |x: f32| Ray {
        origin: Vec3::new(x, 0.0, 0.0),
        dir: Vec3::NEG_Z,
        t_min: 0.0,
        t_max: f32::INFINITY,
    };
    assert_eq!(
        accel.intersect(&ray(-3.0)).map(|h| (h.instance, h.t)),
        Some((0, 5.0))
    );
    assert_eq!(
        accel.intersect(&ray(3.0)).map(|h| (h.instance, h.t)),
        Some((1, 5.0))
    );
    assert_eq!(accel.intersect(&ray(0.0)), None);
}
