//! `MeshletStats`: what `concordia cluster stats` and `asset.meshlet_stats` report (M4 4.2 Q7).

mod common;

use common::{jobs, patch};
use concordia_cluster::{MeshletBuildSettings, MeshletStats, build};

#[test]
fn stats_summarise_meshlets() {
    let mesh = patch(16, &[0.0], [0.0, 0.0], [0.0; 3]);
    let out = build(&mesh.input(), &MeshletBuildSettings::default(), &jobs(1)).unwrap();
    let bytes = out.encode().unwrap();
    let s = MeshletStats::of(&out, bytes.len());
    assert_eq!((s.source_triangles, s.source_vertices), (512, 289));
    assert_eq!(s.meshlets as usize, out.meshlets.len());
    assert_eq!(
        (s.max_triangles, s.max_vertices, s.cone_weight),
        (128, 128, 0.25)
    );
    let tris: Vec<f32> = out
        .meshlets
        .iter()
        .map(|m| f32::from(m.triangle_count))
        .collect();
    assert_eq!(
        s.triangles_per_meshlet.min,
        tris.iter().copied().fold(f32::MAX, f32::min)
    );
    assert_eq!(
        s.triangles_per_meshlet.max,
        tris.iter().copied().fold(0.0, f32::max)
    );
    assert!((s.triangles_per_meshlet.mean - 512.0 / tris.len() as f32).abs() < 1e-4);
    assert!((s.triangle_fill - s.triangles_per_meshlet.mean / 128.0).abs() < 1e-6);
    assert!(s.vertices_per_meshlet.min >= 3.0 && s.vertices_per_meshlet.max <= 128.0);
    assert!(s.radius.min > 0.0 && s.radius.min <= s.radius.mean && s.radius.mean <= s.radius.max);
    assert_eq!(s.cullable_share, 1.0, "a flat patch: every cone can cull");
    let angle = s.mean_cone_half_angle_deg.unwrap();
    assert!(
        (0.0..10.0).contains(&angle),
        "flat: a narrow cone ({angle}°)"
    );
    assert_eq!(s.encoded_bytes, bytes.len() as u64);
}

#[test]
fn an_empty_mesh_has_empty_stats() {
    let s = MeshletStats::of(&concordia_cluster::MeshletMesh::default(), 48);
    assert_eq!(s.meshlets, 0);
    assert_eq!(s.triangles_per_meshlet.mean, 0.0);
    assert_eq!(s.cullable_share, 0.0);
    assert_eq!(s.mean_cone_half_angle_deg, None);
}
