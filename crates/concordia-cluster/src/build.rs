//! The build: material groups, meshoptimizer, per-meshlet optimization and bounds.

use std::collections::BTreeMap;

use concordia_core::jobs::JobSystem;
use meshopt_sys::safe;

use crate::{
    ClusterError, ClusterInput, Meshlet, MeshletBuildSettings, MeshletMesh, NormalCone, Sphere,
};

/// Splits `input` into meshlets; the output is identical for any number of threads.
pub fn build(
    input: &ClusterInput<'_>,
    settings: &MeshletBuildSettings,
    _jobs: &JobSystem,
) -> Result<MeshletMesh, ClusterError> {
    let mut out = MeshletMesh {
        source_triangles: u32::try_from(input.triangle_count())
            .map_err(|_| ClusterError::Input("more than 2^32 triangles".into()))?,
        ..MeshletMesh::default()
    };
    for (material, indices) in material_groups(input) {
        let part = build_group(&indices, input.positions(), material, settings)?;
        append(&mut out, part);
    }
    Ok(out)
}

/// Each material's triangle list, in ascending material order.
fn material_groups(input: &ClusterInput<'_>) -> BTreeMap<u16, Vec<u32>> {
    let mut groups: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
    for (t, tri) in input.indices().as_chunks::<3>().0.iter().enumerate() {
        groups
            .entry(input.material(t))
            .or_default()
            .extend_from_slice(tri);
    }
    groups
}

/// One material's meshlets, optimized and bounded, with offsets local to this part.
fn build_group(
    indices: &[u32],
    positions: &[[f32; 3]],
    material: u16,
    settings: &MeshletBuildSettings,
) -> Result<MeshletMesh, ClusterError> {
    let mut built = safe::build_meshlets(
        indices,
        positions,
        settings.max_vertices() as usize,
        settings.size().triangles() as usize,
        settings.cone_weight(),
    )?;
    let mut meshlets = Vec::with_capacity(built.meshlets.len());
    for m in &built.meshlets {
        let v = m.vertex_offset as usize..(m.vertex_offset + m.vertex_count) as usize;
        let t = m.triangle_offset as usize..(m.triangle_offset + 3 * m.triangle_count) as usize;
        safe::optimize_meshlet(
            &mut built.vertices[v.clone()],
            &mut built.triangles[t.clone()],
        )?;
        let b = safe::meshlet_bounds(&built.vertices[v.clone()], &built.triangles[t], positions)?;
        debug_assert!(m.vertex_count <= 128 && m.triangle_count <= 128);
        let bounds = if b.radius == 0.0 && b.cone_cutoff_s8 == 0 {
            // meshoptimizer zeroes the bounds when every triangle has zero area.
            enclosing_sphere(&built.vertices[v], positions)
        } else {
            Sphere {
                center: b.center,
                radius: b.radius,
            }
        };
        meshlets.push(Meshlet {
            bounds,
            cone: NormalCone {
                axis: b.cone_axis_s8,
                cutoff: b.cone_cutoff_s8,
            },
            material,
            vertex_offset: m.vertex_offset,
            vertex_count: m.vertex_count as u8,
            triangle_offset: m.triangle_offset,
            triangle_count: m.triangle_count as u8,
        });
    }
    Ok(MeshletMesh {
        meshlets,
        vertices: built.vertices,
        triangles: built.triangles,
        source_triangles: 0,
    })
}

/// A sphere around `vertices` (the box's center, the farthest vertex's distance).
fn enclosing_sphere(vertices: &[u32], positions: &[[f32; 3]]) -> Sphere {
    let points = || vertices.iter().map(|&v| positions[v as usize]);
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in points() {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let center = [0, 1, 2].map(|i| 0.5 * (lo[i] + hi[i]));
    let radius = points()
        .map(|p| {
            let d = [0, 1, 2].map(|i| p[i] - center[i]);
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })
        .fold(0.0f32, f32::max);
    Sphere { center, radius }
}

/// Appends `part`, rebasing its offsets onto `out`'s arrays.
fn append(out: &mut MeshletMesh, part: MeshletMesh) {
    let (v0, t0) = (out.vertices.len() as u32, out.triangles.len() as u32);
    out.meshlets.extend(part.meshlets.into_iter().map(|mut m| {
        m.vertex_offset += v0;
        m.triangle_offset += t0;
        m
    }));
    out.vertices.extend(part.vertices);
    out.triangles.extend(part.triangles);
}
