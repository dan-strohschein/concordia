//! The build's invariants over random meshes (M4 4.2 tests first).

mod common;

use std::collections::BTreeMap;

use common::{Mesh, any_mesh, canonical, jobs, patch, soup};
use concordia_cluster::{ClusterInput, MeshletBuildSettings, MeshletSize, build};
use proptest::prelude::*;

fn settings_strategy() -> impl Strategy<Value = MeshletBuildSettings> {
    (
        prop_oneof![Just(MeshletSize::S64), Just(MeshletSize::S128)],
        64u32..=128,
        0.0f32..=1.0,
    )
        .prop_map(|(s, v, w)| MeshletBuildSettings::new(s, v, w).unwrap())
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

proptest! {
    #[test]
    fn meshlets_respect_vertex_and_triangle_limits(mesh in any_mesh(), settings in settings_strategy()) {
        let out = build(&mesh.input(), &settings, &jobs(2)).unwrap();
        for m in &out.meshlets {
            prop_assert!(u32::from(m.triangle_count) <= settings.size().triangles());
            prop_assert!(u32::from(m.vertex_count) <= settings.max_vertices());
            // Degenerate triangles may leave a meshlet with fewer than 3 vertices.
            prop_assert!(m.triangle_count > 0 && m.vertex_count >= 1);
            prop_assert!(m.vertex_range().end <= out.vertices.len());
            prop_assert!(m.triangle_range().end <= out.triangles.len());
            let local = &out.triangles[m.triangle_range()];
            prop_assert!(local.iter().all(|&c| c < m.vertex_count));
        }
    }

    #[test]
    fn every_source_triangle_appears_in_exactly_one_meshlet(mesh in any_mesh()) {
        let out = build(&mesh.input(), &MeshletBuildSettings::default(), &jobs(3)).unwrap();
        let mut want: BTreeMap<[u32; 3], i64> = BTreeMap::new();
        for t in mesh.indices.as_chunks::<3>().0 {
            *want.entry(canonical([t[0], t[1], t[2]])).or_default() += 1;
        }
        for m in &out.meshlets {
            for t in out.triangles_of(m) {
                *want.entry(canonical(t)).or_default() -= 1;
            }
        }
        prop_assert!(want.values().all(|&c| c == 0), "triangle counts differ: {want:?}");
        prop_assert_eq!(out.source_triangles as usize, mesh.indices.len() / 3);
    }

    #[test]
    fn meshlets_never_span_two_materials(mesh in soup()) {
        let input = mesh.input();
        let out = build(&input, &MeshletBuildSettings::default(), &jobs(2)).unwrap();
        // Map each source triangle (canonical, with multiplicity) to its materials.
        let mut by_tri: BTreeMap<[u32; 3], Vec<u16>> = BTreeMap::new();
        for (t, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
            by_tri.entry(canonical([tri[0], tri[1], tri[2]])).or_default().push(input.material(t));
        }
        let mut last = 0u16;
        for m in &out.meshlets {
            prop_assert!(m.material >= last, "meshlets are grouped by ascending material");
            last = m.material;
            for t in out.triangles_of(m) {
                let mats = by_tri.get_mut(&canonical(t)).unwrap();
                let i = mats.iter().position(|&x| x == m.material);
                prop_assert!(i.is_some(), "triangle {t:?} isn't material {}", m.material);
                mats.remove(i.unwrap());
            }
        }
    }

    #[test]
    fn meshlet_bounds_contain_every_vertex(mesh in any_mesh()) {
        let out = build(&mesh.input(), &MeshletBuildSettings::default(), &jobs(2)).unwrap();
        for m in &out.meshlets {
            let s = m.bounds;
            for &v in &out.vertices[m.vertex_range()] {
                let d = len(sub(mesh.positions[v as usize], s.center));
                prop_assert!(d <= s.radius * (1.0 + 1e-5) + 1e-5, "vertex {v} at {d} > {}", s.radius);
            }
        }
    }

    #[test]
    fn normal_cone_is_conservative(
        mesh in any_mesh(),
        eyes in prop::collection::vec(prop::array::uniform3(-20.0f32..20.0), 24),
    ) {
        let out = build(&mesh.input(), &MeshletBuildSettings::default(), &jobs(2)).unwrap();
        for m in &out.meshlets {
            for &eye in &eyes {
                if !m.cone.rejects(&m.bounds, eye) {
                    continue;
                }
                for [a, b, c] in out.triangles_of(m) {
                    let [pa, pb, pc] = [a, b, c].map(|i| mesh.positions[i as usize]);
                    let n = cross(sub(pb, pa), sub(pc, pa));
                    if len(n) < 1e-12 {
                        continue; // degenerate: faces nowhere
                    }
                    let to_eye = sub(eye, pa);
                    let facing = dot(n, to_eye) / (len(n) * len(to_eye));
                    prop_assert!(facing <= 1e-4, "rejected meshlet has a front face ({facing}) from {eye:?}");
                }
            }
        }
    }
}

/// The conservative-cone test above isn't vacuous: a flat patch seen from behind is culled.
#[test]
fn cone_test_rejects_a_back_facing_patch() {
    let mesh = patch(16, &[0.0], [0.0, 0.0], [0.0, 0.0, 0.0]);
    let out = build(&mesh.input(), &MeshletBuildSettings::default(), &jobs(1)).unwrap();
    assert!(!out.meshlets.is_empty());
    for m in &out.meshlets {
        assert!(m.cone.can_cull());
        assert!(
            m.cone.rejects(&m.bounds, [0.8, 0.8, -5.0]),
            "seen from below"
        );
        assert!(
            !m.cone.rejects(&m.bounds, [0.8, 0.8, 5.0]),
            "seen from above"
        );
    }
}

/// meshoptimizer zeroes the bounds of a meshlet whose triangles all have zero area; ours still
/// contain its vertices and its cone never rejects.
#[test]
fn a_meshlet_of_zero_area_triangles_still_has_enclosing_bounds() {
    let mesh = Mesh {
        positions: vec![[1.0, 2.0, 3.0], [2.0, 2.0, 3.0], [3.0, 2.0, 3.0]],
        indices: vec![0, 1, 2, 2, 1, 0],
        materials: None,
    };
    let out = build(&mesh.input(), &MeshletBuildSettings::default(), &jobs(1)).unwrap();
    let m = out.meshlets[0];
    assert!(
        (m.bounds.center[0] - 2.0).abs() < 1e-6 && m.bounds.radius >= 1.0,
        "{:?}",
        m.bounds
    );
    assert!(!m.cone.rejects(&m.bounds, [2.0, 10.0, 3.0]));
}

#[test]
fn an_empty_mesh_has_no_meshlets() {
    let mesh = Mesh {
        positions: vec![],
        indices: vec![],
        materials: None,
    };
    let out = build(&mesh.input(), &MeshletBuildSettings::default(), &jobs(1)).unwrap();
    assert!(out.meshlets.is_empty() && out.vertices.is_empty() && out.triangles.is_empty());
}

#[test]
fn input_is_validated() {
    let p = [[0.0f32; 3]; 3];
    assert!(
        ClusterInput::new(&p, &[0, 1], None).is_err(),
        "not a triangle list"
    );
    assert!(
        ClusterInput::new(&p, &[0, 1, 3], None).is_err(),
        "index out of range"
    );
    assert!(
        ClusterInput::new(&p, &[0, 1, 2], Some(&[])).is_err(),
        "one material per triangle"
    );
    let nan = [[0.0, f32::NAN, 0.0], [0.0; 3], [0.0; 3]];
    assert!(
        ClusterInput::new(&nan, &[0, 1, 2], None).is_err(),
        "finite positions"
    );
    assert!(ClusterInput::new(&p, &[0, 1, 2], Some(&[7])).is_ok());
}
