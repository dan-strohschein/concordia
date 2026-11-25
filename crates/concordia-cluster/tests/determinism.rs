//! Piles and threads: the same bytes for any thread count (M4 4.2 Q5).

mod common;

use std::collections::BTreeMap;

use common::{Mesh, canonical, jobs, patch};
use concordia_cluster::{MeshletBuildSettings, PILE_TRIANGLES, build, piles};
use meshopt_sys::safe;

/// A patch with more than two piles' worth of triangles, plus a second material on every 7th.
fn big_mesh() -> Mesh {
    let heights: Vec<f32> = (0..97)
        .map(|i| ((i * 37) % 23) as f32 / 11.0 - 1.0)
        .collect();
    let mut mesh = patch(400, &heights, [0.4, 1.1], [0.0, 0.0, 0.0]);
    let triangles = mesh.indices.len() / 3;
    assert!(triangles > 2 * PILE_TRIANGLES as usize);
    mesh.materials = Some((0..triangles).map(|t| u16::from(t % 7 == 0)).collect());
    mesh
}

#[test]
fn build_is_deterministic_across_thread_counts() {
    let mesh = big_mesh();
    let settings = MeshletBuildSettings::default();
    let one = build(&mesh.input(), &settings, &jobs(1)).unwrap();
    for threads in [2, 8] {
        let many = build(&mesh.input(), &settings, &jobs(threads)).unwrap();
        assert!(one == many, "{threads} threads differ from 1");
    }
    // Still every triangle exactly once, and within the limits.
    let mut want: BTreeMap<[u32; 3], i64> = BTreeMap::new();
    for t in mesh.indices.as_chunks::<3>().0 {
        *want.entry(canonical(*t)).or_default() += 1;
    }
    for m in &one.meshlets {
        assert!(m.triangle_count <= 128 && m.vertex_count <= 128);
        for t in one.triangles_of(m) {
            *want.entry(canonical(t)).or_default() -= 1;
        }
    }
    assert!(want.values().all(|&c| c == 0));
}

#[test]
fn small_meshes_match_meshoptimizers_unsplit_output() {
    let heights: Vec<f32> = (0..31).map(|i| (i % 5) as f32 * 0.3).collect();
    let mesh = patch(60, &heights, [0.2, 0.7], [1.0, 2.0, 3.0]);
    let settings = MeshletBuildSettings::default();
    let out = build(&mesh.input(), &settings, &jobs(4)).unwrap();
    let mut direct = safe::build_meshlets(
        &mesh.indices,
        &mesh.positions,
        128,
        128,
        settings.cone_weight(),
    )
    .unwrap();
    for m in &direct.meshlets {
        let v = m.vertex_offset as usize..(m.vertex_offset + m.vertex_count) as usize;
        let t = m.triangle_offset as usize..(m.triangle_offset + 3 * m.triangle_count) as usize;
        safe::optimize_meshlet(&mut direct.vertices[v], &mut direct.triangles[t]).unwrap();
    }
    assert_eq!(out.vertices, direct.vertices);
    assert_eq!(out.triangles, direct.triangles);
    assert_eq!(out.meshlets.len(), direct.meshlets.len());
}

/// Triangle `k` of the scrambled patch is the original patch's triangle `k·7919 mod T`.
fn scrambled(mesh: &Mesh) -> (Mesh, Vec<u32>) {
    let t = (mesh.indices.len() / 3) as u64;
    let original: Vec<u32> = (0..t).map(|k| (k * 7919 % t) as u32).collect();
    let indices = original
        .iter()
        .flat_map(|&o| {
            mesh.indices[3 * o as usize..3 * o as usize + 3]
                .iter()
                .copied()
        })
        .collect();
    let materials = mesh
        .materials
        .as_ref()
        .map(|m| original.iter().map(|&o| m[o as usize]).collect());
    (
        Mesh {
            positions: mesh.positions.clone(),
            indices,
            materials,
        },
        original,
    )
}

#[test]
fn large_materials_are_split_into_spatial_piles() {
    let (mesh, original) = scrambled(&big_mesh());
    let input = mesh.input();
    let piles = piles(&input);
    // Material 0 has ~6/7 of the triangles (3 piles), material 1 the rest (1 pile).
    let mats: Vec<u16> = piles.iter().map(|p| p.material).collect();
    assert_eq!(mats, [0, 0, 0, 1]);
    let mut seen = vec![0u8; input.triangle_count()];
    for p in &piles {
        assert!(p.triangles.len() <= PILE_TRIANGLES as usize);
        for &t in &p.triangles {
            assert_eq!(input.material(t as usize), p.material);
            seen[t as usize] += 1;
        }
    }
    assert!(
        seen.iter().all(|&n| n == 1),
        "every triangle in exactly one pile"
    );
    // Spatial: in the patch's own grid (2 triangles per cell, 400 cells a row), material 0's
    // piles together span about two patches' worth of bounding boxes; piles cut in the
    // scrambled index order would each span all of it (3.0).
    let share = |tris: &[u32]| {
        let (mut lo, mut hi) = ([u32::MAX; 2], [0u32; 2]);
        for &t in tris {
            let o = original[t as usize] / 2;
            let cell = [o % 400, o / 400];
            for i in 0..2 {
                lo[i] = lo[i].min(cell[i]);
                hi[i] = hi[i].max(cell[i]);
            }
        }
        f64::from(hi[0] - lo[0] + 1) * f64::from(hi[1] - lo[1] + 1) / (400.0 * 400.0)
    };
    let total: f64 = piles[..3].iter().map(|p| share(&p.triangles)).sum();
    assert!(total < 2.2, "piles span {total} patches");
}
