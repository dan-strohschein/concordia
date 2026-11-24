//! Shapes swept around Y or along Z: cylinders, capsules, tori, lathes and extrusions.

use std::f32::consts::{PI, TAU};

use concordia_assets::MeshData;

use super::build::Builder;
use crate::error::GenError;

/// A profile point: (radius, y) and its outward normal in the (radius, y) plane.
type ProfilePoint = ([f32; 2], [f32; 2]);

/// Revolves `rings` (each a run of profile points, joined in order) around Y in `segments`.
fn revolve(rings: &[Vec<ProfilePoint>], segments: u32) -> MeshData {
    let mut b = Builder::default();
    for ring in rings {
        let mut rows = Vec::new();
        for (i, ([r, y], [nr, ny])) in ring.iter().enumerate() {
            let row: Vec<u32> = (0..=segments)
                .map(|s| {
                    let phi = TAU * s as f32 / segments as f32;
                    let (sin, cos) = phi.sin_cos();
                    b.vertex(
                        [r * cos, *y, r * sin],
                        [nr * cos, *ny, nr * sin],
                        [
                            s as f32 / segments as f32,
                            i as f32 / (ring.len() - 1).max(1) as f32,
                        ],
                    )
                })
                .collect();
            rows.push(row);
        }
        for (i, pair) in rows.windows(2).enumerate() {
            // A row on the axis is one point: a fan of triangles, not quads.
            let (low, high) = (on_axis(ring[i].0), on_axis(ring[i + 1].0));
            for s in 0..segments as usize {
                let (a, b2, c, d) = (pair[0][s], pair[0][s + 1], pair[1][s + 1], pair[1][s]);
                match (low, high) {
                    (true, true) => {}
                    (true, false) => b.triangle(a, c, d),
                    (false, true) => b.triangle(a, b2, c),
                    (false, false) => b.quad(a, b2, c, d),
                }
            }
        }
    }
    b.finish()
}

/// Whether a profile point lies on the Y axis.
fn on_axis(p: [f32; 2]) -> bool {
    p[0] <= 1e-6
}

/// Flat-shaded bands between consecutive profile points (each band its own normal).
fn flat_bands(profile: &[[f32; 2]]) -> Vec<Vec<ProfilePoint>> {
    profile
        .windows(2)
        .filter(|w| w[0] != w[1])
        .map(|w| {
            let (dr, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            let n = [dy, -dr];
            vec![(w[0], n), (w[1], n)]
        })
        .collect()
}

pub(crate) fn segments_ok(field: &str, n: u32, least: u32) -> Result<(), GenError> {
    if n < least {
        return Err(GenError::invalid(
            field,
            format!("{n} is fewer than {least}"),
        ));
    }
    if n > 4096 {
        return Err(GenError::invalid(field, format!("{n} is more than 4096")));
    }
    Ok(())
}

pub(crate) fn positive(field: &str, v: f32) -> Result<(), GenError> {
    if !(v.is_finite() && v > 0.0) {
        return Err(GenError::invalid(
            field,
            format!("{v} must be a positive number"),
        ));
    }
    Ok(())
}

/// A profile of (radius, y) revolved around Y.
pub(crate) fn lathe(profile: &[[f32; 2]], segments: u32) -> Result<MeshData, GenError> {
    segments_ok("segments", segments, 3)?;
    if profile.len() < 3 {
        return Err(GenError::invalid("profile", "needs at least 3 points"));
    }
    if profile
        .iter()
        .any(|p| !p[0].is_finite() || !p[1].is_finite() || p[0] < 0.0)
    {
        return Err(GenError::invalid("profile", "radii must be finite and ≥ 0"));
    }
    Ok(revolve(&flat_bands(profile), segments))
}

/// A capped cylinder of `radius` and `height` along Y, centred.
pub(crate) fn cylinder(radius: f32, height: f32, segments: u32) -> Result<MeshData, GenError> {
    positive("radius", radius)?;
    positive("height", height)?;
    let h = height / 2.0;
    lathe(&[[0.0, -h], [radius, -h], [radius, h], [0.0, h]], segments)
}

/// A cylinder part of `height` capped by hemispheres of `radius`, along Y, centred.
pub(crate) fn capsule(
    radius: f32,
    height: f32,
    segments: u32,
    rings: u32,
) -> Result<MeshData, GenError> {
    positive("radius", radius)?;
    if !(height.is_finite() && height >= 0.0) {
        return Err(GenError::invalid("height", format!("{height} must be ≥ 0")));
    }
    segments_ok("segments", segments, 3)?;
    segments_ok("rings", rings, 1)?;
    let h = height / 2.0;
    let arc = |from: f32, to: f32, cy: f32| -> Vec<ProfilePoint> {
        (0..=rings)
            .map(|i| {
                let a = from + (to - from) * i as f32 / rings as f32;
                let (s, c) = a.sin_cos();
                // The poles sit exactly on the axis (cos ±π/2 isn't exactly 0 in f32).
                let r = if i == 0 && from < 0.0 || i == rings && to > 0.0 {
                    0.0
                } else {
                    radius * c
                };
                ([r, cy + radius * s], [c, s])
            })
            .collect()
    };
    let mut points = arc(-PI / 2.0, 0.0, -h);
    points.extend(arc(0.0, PI / 2.0, h));
    Ok(revolve(&[points], segments))
}

/// A torus around Y.
pub(crate) fn torus(
    major: f32,
    minor: f32,
    segments: u32,
    sides: u32,
) -> Result<MeshData, GenError> {
    positive("major", major)?;
    positive("minor", minor)?;
    if minor >= major {
        return Err(GenError::invalid("minor", "must be smaller than major"));
    }
    segments_ok("segments", segments, 3)?;
    segments_ok("sides", sides, 3)?;
    let ring: Vec<ProfilePoint> = (0..=sides)
        .map(|i| {
            let a = TAU * i as f32 / sides as f32;
            let (s, c) = a.sin_cos();
            ([major + minor * c, minor * s], [c, s])
        })
        .collect();
    Ok(revolve(&[ring], segments))
}

/// A counter-clockwise (or clockwise, reversed) simple polygon in XY extruded along +Z.
pub(crate) fn extrude(profile: &[[f32; 2]], depth: f32) -> Result<MeshData, GenError> {
    positive("depth", depth)?;
    if profile.len() < 3 || profile.iter().flatten().any(|c| !c.is_finite()) {
        return Err(GenError::invalid(
            "profile",
            "needs at least 3 finite points",
        ));
    }
    let mut poly = profile.to_vec();
    if area(&poly) < 0.0 {
        poly.reverse();
    }
    if area(&poly).abs() < 1e-12 {
        return Err(GenError::invalid("profile", "encloses no area"));
    }
    let triangles =
        ear_clip(&poly).ok_or_else(|| GenError::invalid("profile", "is not a simple polygon"))?;
    let mut b = Builder::default();
    for (z, nz) in [(0.0, -1.0), (depth, 1.0)] {
        let ids: Vec<u32> = poly
            .iter()
            .map(|p| b.vertex([p[0], p[1], z], [0.0, 0.0, nz], *p))
            .collect();
        for [i, j, k] in &triangles {
            b.triangle(ids[*i], ids[*j], ids[*k]);
        }
    }
    let n = poly.len();
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let normal = [q[1] - p[1], p[0] - q[0], 0.0];
        let v = |pt: [f32; 2], z: f32, u: f32| ([pt[0], pt[1], z], u);
        let quad = [
            v(p, 0.0, 0.0),
            v(q, 0.0, 1.0),
            v(q, depth, 1.0),
            v(p, depth, 0.0),
        ];
        let ids: Vec<u32> = quad
            .iter()
            .map(|(pos, u)| b.vertex(*pos, normal, [*u, pos[2] / depth]))
            .collect();
        b.quad(ids[0], ids[1], ids[2], ids[3]);
    }
    Ok(b.finish())
}

/// Twice the signed area (positive: counter-clockwise).
fn area(poly: &[[f32; 2]]) -> f32 {
    (0..poly.len())
        .map(|i| {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum()
}

/// Ear clipping of a counter-clockwise simple polygon into triangles (indices into it).
fn ear_clip(poly: &[[f32; 2]]) -> Option<Vec<[usize; 3]>> {
    let mut left: Vec<usize> = (0..poly.len()).collect();
    let mut out = Vec::new();
    let turn = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    while left.len() > 3 {
        let n = left.len();
        let ear = (0..n).find(|&i| {
            let (a, b, c) = (left[(i + n - 1) % n], left[i], left[(i + 1) % n]);
            let (pa, pb, pc) = (poly[a], poly[b], poly[c]);
            turn(pa, pb, pc) > 0.0
                && left.iter().all(|&o| {
                    o == a || o == b || o == c || {
                        let p = poly[o];
                        !(turn(pa, pb, p) >= 0.0
                            && turn(pb, pc, p) >= 0.0
                            && turn(pc, pa, p) >= 0.0)
                    }
                })
        })?;
        out.push([left[(ear + n - 1) % n], left[ear], left[(ear + 1) % n]]);
        left.remove(ear);
    }
    out.push([left[0], left[1], left[2]]);
    Some(out)
}
