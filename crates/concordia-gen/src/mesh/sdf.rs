//! Signed-distance shapes meshed by surface nets (Gibson, "Constrained Elastic Surface Nets",
//! 1998): one vertex per grid cell the surface crosses, at the mean of its edge crossings, and a
//! quad across every grid edge where the sign changes.

use std::collections::BTreeMap;

use concordia_assets::MeshData;

use super::SdfShape;
use super::build::{Builder, normalize};
use super::sweep::positive;
use crate::error::GenError;

/// At most this many grid points per axis.
const MAX_POINTS: usize = 256;

/// The signed distance at `p` of `shapes`' union, smoothed over `k`.
fn distance(shapes: &[SdfShape], k: f32, p: [f32; 3]) -> f32 {
    shapes
        .iter()
        .map(|s| shape(s, p))
        .reduce(|a, b| smooth_min(a, b, k))
        .unwrap_or(f32::MAX)
}

fn shape(s: &SdfShape, p: [f32; 3]) -> f32 {
    match s {
        SdfShape::Sphere { center, radius } => {
            let d = [p[0] - center[0], p[1] - center[1], p[2] - center[2]];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - radius
        }
        SdfShape::Box { center, half } => {
            let q: [f32; 3] = std::array::from_fn(|i| (p[i] - center[i]).abs() - half[i]);
            let outside = q.map(|c| c.max(0.0));
            let out_len =
                (outside[0] * outside[0] + outside[1] * outside[1] + outside[2] * outside[2])
                    .sqrt();
            out_len + q[0].max(q[1]).max(q[2]).min(0.0)
        }
    }
}

/// A polynomial smooth minimum (Quílez); `k = 0` is the plain minimum.
fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

fn bounds(s: &SdfShape) -> ([f32; 3], [f32; 3]) {
    match s {
        SdfShape::Sphere { center, radius } => {
            (center.map(|c| c - radius), center.map(|c| c + radius))
        }
        SdfShape::Box { center, half } => (
            std::array::from_fn(|i| center[i] - half[i]),
            std::array::from_fn(|i| center[i] + half[i]),
        ),
    }
}

fn check(shapes: &[SdfShape], smooth: f32, cell: f32) -> Result<([f32; 3], [usize; 3]), GenError> {
    if shapes.is_empty() {
        return Err(GenError::invalid("shapes", "needs at least one shape"));
    }
    for s in shapes {
        match s {
            SdfShape::Sphere { radius, .. } => positive("radius", *radius)?,
            SdfShape::Box { half, .. } => half.iter().try_for_each(|h| positive("half", *h))?,
        }
    }
    if !(smooth.is_finite() && smooth >= 0.0) {
        return Err(GenError::invalid("smooth", "must be ≥ 0"));
    }
    positive("cell", cell)?;
    let (mut lo, mut hi) = bounds(&shapes[0]);
    for s in &shapes[1..] {
        let (a, b) = bounds(s);
        lo = std::array::from_fn(|i| lo[i].min(a[i]));
        hi = std::array::from_fn(|i| hi[i].max(b[i]));
    }
    let extent = (0..3).map(|i| hi[i] - lo[i]).fold(0.0f32, f32::max);
    if cell > extent / 10.0 {
        return Err(GenError::invalid(
            "cell",
            format!("{cell} is coarser than a tenth of the shapes' extent ({extent})"),
        ));
    }
    let margin = smooth + 2.0 * cell;
    let origin = lo.map(|c| c - margin);
    let counts: [usize; 3] =
        std::array::from_fn(|i| ((hi[i] - lo[i] + 2.0 * margin) / cell).ceil() as usize + 1);
    if counts.iter().any(|&n| n > MAX_POINTS) {
        return Err(GenError::invalid(
            "cell",
            format!("{cell} needs more than {MAX_POINTS} grid points per axis"),
        ));
    }
    Ok((origin, counts))
}

/// The union of `shapes` (smoothed over `smooth`), meshed at `cell`.
pub(crate) fn sdf(shapes: &[SdfShape], smooth: f32, cell: f32) -> Result<MeshData, GenError> {
    let (origin, [nx, ny, nz]) = check(shapes, smooth, cell)?;
    let at = |i: usize, j: usize, k: usize| {
        [
            origin[0] + i as f32 * cell,
            origin[1] + j as f32 * cell,
            origin[2] + k as f32 * cell,
        ]
    };
    let index = |i: usize, j: usize, k: usize| (k * ny + j) * nx + i;
    let mut field = vec![0.0f32; nx * ny * nz];
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                field[index(i, j, k)] = distance(shapes, smooth, at(i, j, k));
            }
        }
    }
    let gradient = |p: [f32; 3]| {
        let e = cell * 0.5;
        let d = |o: [f32; 3]| distance(shapes, smooth, [p[0] + o[0], p[1] + o[1], p[2] + o[2]]);
        normalize([
            d([e, 0.0, 0.0]) - d([-e, 0.0, 0.0]),
            d([0.0, e, 0.0]) - d([0.0, -e, 0.0]),
            d([0.0, 0.0, e]) - d([0.0, 0.0, -e]),
        ])
    };
    let mut b = Builder::default();
    let mut cells: BTreeMap<(usize, usize, usize), u32> = BTreeMap::new();
    let corners = [
        (0, 0, 0),
        (1, 0, 0),
        (0, 1, 0),
        (1, 1, 0),
        (0, 0, 1),
        (1, 0, 1),
        (0, 1, 1),
        (1, 1, 1),
    ];
    let edges = [
        (0, 1),
        (2, 3),
        (4, 5),
        (6, 7),
        (0, 2),
        (1, 3),
        (4, 6),
        (5, 7),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    for k in 0..nz - 1 {
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                let v: Vec<f32> = corners
                    .iter()
                    .map(|&(a, b2, c)| field[index(i + a, j + b2, k + c)])
                    .collect();
                if v.iter().all(|d| *d < 0.0) || v.iter().all(|d| *d >= 0.0) {
                    continue;
                }
                let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
                for &(e0, e1) in &edges {
                    if (v[e0] < 0.0) == (v[e1] < 0.0) {
                        continue;
                    }
                    let t = v[e0] / (v[e0] - v[e1]);
                    let (a, c) = (corners[e0], corners[e1]);
                    let p0 = at(i + a.0, j + a.1, k + a.2);
                    let p1 = at(i + c.0, j + c.1, k + c.2);
                    for d in 0..3 {
                        sum[d] += p0[d] + t * (p1[d] - p0[d]);
                    }
                    n += 1.0;
                }
                let p = sum.map(|c| c / n);
                let uv = [
                    (p[0] - origin[0]) / (nx as f32 * cell),
                    (p[2] - origin[2]) / (nz as f32 * cell),
                ];
                cells.insert((i, j, k), b.vertex(p, gradient(p), uv));
            }
        }
    }
    let quad = |b: &mut Builder, ids: [(usize, usize, usize); 4]| {
        let v: Vec<u32> = ids.iter().filter_map(|c| cells.get(c).copied()).collect();
        if let [a, b2, c, d] = v[..] {
            b.quad(a, b2, c, d);
        }
    };
    for k in 1..nz - 1 {
        for j in 1..ny - 1 {
            for i in 1..nx - 1 {
                let here = field[index(i, j, k)] < 0.0;
                if i + 1 < nx && here != (field[index(i + 1, j, k)] < 0.0) {
                    quad(
                        &mut b,
                        [(i, j - 1, k - 1), (i, j, k - 1), (i, j, k), (i, j - 1, k)],
                    );
                }
                if j + 1 < ny && here != (field[index(i, j + 1, k)] < 0.0) {
                    quad(
                        &mut b,
                        [(i - 1, j, k - 1), (i, j, k - 1), (i, j, k), (i - 1, j, k)],
                    );
                }
                if k + 1 < nz && here != (field[index(i, j, k + 1)] < 0.0) {
                    quad(
                        &mut b,
                        [(i - 1, j - 1, k), (i, j - 1, k), (i, j, k), (i - 1, j, k)],
                    );
                }
            }
        }
    }
    Ok(b.finish())
}
