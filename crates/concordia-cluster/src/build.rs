//! The build: material groups, meshoptimizer, per-meshlet optimization and bounds.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use concordia_core::jobs::JobSystem;
use meshopt_sys::safe;

use crate::{
    ClusterError, ClusterInput, Meshlet, MeshletBuildSettings, MeshletMesh, NormalCone, Sphere,
};

/// Triangles per pile: a material with more is split spatially into piles of this size, built
/// in parallel (never derived from the thread count).
pub const PILE_TRIANGLES: u32 = 131_072;

/// One unit of parallel work: some triangles of one material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pile {
    /// The material.
    pub material: u16,
    /// Source triangle numbers.
    pub triangles: Vec<u32>,
}

/// The piles a build makes, in output order: by material, then by position along a Morton curve.
pub fn piles(input: &ClusterInput<'_>) -> Vec<Pile> {
    let mut groups: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
    for t in 0..input.triangle_count() {
        groups.entry(input.material(t)).or_default().push(t as u32);
    }
    let pile = PILE_TRIANGLES as usize;
    let mut out = Vec::new();
    for (material, mut triangles) in groups {
        if triangles.len() > pile {
            morton_order(input, &mut triangles);
        }
        out.extend(triangles.chunks(pile).map(|c| Pile {
            material,
            triangles: c.to_vec(),
        }));
    }
    out
}

/// Sorts triangles by the Morton code of their centroids (ties by triangle number).
fn morton_order(input: &ClusterInput<'_>, triangles: &mut [u32]) {
    let centroid = |t: u32| {
        let i = &input.indices()[3 * t as usize..3 * t as usize + 3];
        let [a, b, c] = [0, 1, 2].map(|k| input.positions()[i[k] as usize]);
        [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0)
    };
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for &t in triangles.iter() {
        let c = centroid(t);
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    let scale = [0, 1, 2].map(|k| {
        let extent = hi[k] - lo[k];
        if extent > 0.0 {
            MORTON_MAX / extent
        } else {
            0.0
        }
    });
    let code = |t: u32| {
        let c = centroid(t);
        let q = [0, 1, 2].map(|k| ((c[k] - lo[k]) * scale[k]).clamp(0.0, MORTON_MAX) as u64);
        spread(q[0]) | spread(q[1]) << 1 | spread(q[2]) << 2
    };
    let mut keyed: Vec<(u64, u32)> = triangles.iter().map(|&t| (code(t), t)).collect();
    keyed.sort_unstable();
    for (slot, (_, t)) in triangles.iter_mut().zip(keyed) {
        *slot = t;
    }
}

/// The largest 21-bit coordinate.
const MORTON_MAX: f32 = ((1 << 21) - 1) as f32;

/// Spreads 21 bits so two zero bits follow each one.
fn spread(v: u64) -> u64 {
    let mut x = v & 0x1f_ffff;
    x = (x | x << 32) & 0x1f_0000_0000_ffff;
    x = (x | x << 16) & 0x1f_0000_ff00_00ff;
    x = (x | x << 8) & 0x100f_00f0_0f00_f00f;
    x = (x | x << 4) & 0x10c3_0c30_c30c_30c3;
    (x | x << 2) & 0x1249_2492_4924_9249
}

/// Splits `input` into meshlets; the output is identical for any number of threads.
pub fn build(
    input: &ClusterInput<'_>,
    settings: &MeshletBuildSettings,
    jobs: &JobSystem,
) -> Result<MeshletMesh, ClusterError> {
    let source_triangles = u32::try_from(input.triangle_count())
        .map_err(|_| ClusterError::Input("more than 2^32 triangles".into()))?;
    let mut work: Vec<(Pile, Option<Result<MeshletMesh, ClusterError>>)> =
        piles(input).into_iter().map(|p| (p, None)).collect();
    // One pile per chunk: results land in their pile's slot, so order never depends on threads.
    jobs.par_chunks_mut(&mut work, NonZeroUsize::MIN, |_, slot| {
        for (pile, result) in slot {
            let indices = pile_indices(input, &pile.triangles);
            *result = Some(build_group(
                &indices,
                input.positions(),
                pile.material,
                settings,
            ));
        }
    });
    let mut out = MeshletMesh {
        source_triangles,
        source_vertices: u32::try_from(input.positions().len())
            .map_err(|_| ClusterError::Input("more than 2^32 vertices".into()))?,
        settings: *settings,
        ..MeshletMesh::default()
    };
    for (_, result) in work {
        append(&mut out, result.expect("every pile ran")?);
    }
    Ok(out)
}

/// The triangle list of `triangles`.
fn pile_indices(input: &ClusterInput<'_>, triangles: &[u32]) -> Vec<u32> {
    let all = input.indices();
    triangles
        .iter()
        .flat_map(|&t| all[3 * t as usize..3 * t as usize + 3].iter().copied())
        .collect()
}

/// One pile's meshlets, optimized, bounded and with 4-byte aligned triangle runs; offsets are
/// local to this part.
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
    let mut triangles = Vec::with_capacity(built.triangles.len() + 3 * built.meshlets.len());
    for m in &built.meshlets {
        let v = m.vertex_offset as usize..(m.vertex_offset + m.vertex_count) as usize;
        let t = m.triangle_offset as usize..(m.triangle_offset + 3 * m.triangle_count) as usize;
        let (verts, tris) = (&mut built.vertices[v], &mut built.triangles[t]);
        safe::optimize_meshlet(verts, tris)?;
        debug_assert!(m.vertex_count <= 128 && m.triangle_count <= 128);
        let (bounds, cone) = bounds_and_cone(verts, tris, positions)?;
        meshlets.push(Meshlet {
            bounds,
            cone,
            material,
            vertex_offset: m.vertex_offset,
            vertex_count: m.vertex_count as u8,
            triangle_offset: triangles.len() as u32,
            triangle_count: m.triangle_count as u8,
        });
        triangles.extend_from_slice(tris);
        triangles.resize(triangles.len().next_multiple_of(4), 0);
    }
    Ok(MeshletMesh {
        meshlets,
        vertices: built.vertices,
        triangles,
        ..MeshletMesh::default()
    })
}

/// A meshlet's sphere and cone from meshoptimizer, enclosing all-zero-area meshlets ourselves.
fn bounds_and_cone(
    vertices: &[u32],
    triangles: &[u8],
    positions: &[[f32; 3]],
) -> Result<(Sphere, NormalCone), ClusterError> {
    let b = safe::meshlet_bounds(vertices, triangles, positions)?;
    let cone = NormalCone {
        axis: b.cone_axis_s8,
        cutoff: b.cone_cutoff_s8,
    };
    let bounds = if b.radius == 0.0 && b.cone_cutoff_s8 == 0 {
        // meshoptimizer zeroes the bounds when every triangle has zero area.
        enclosing_sphere(vertices, positions)
    } else {
        Sphere {
            center: b.center,
            radius: b.radius,
        }
    };
    Ok((bounds, cone))
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
