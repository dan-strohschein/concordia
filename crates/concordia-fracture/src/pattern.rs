//! Seeded patterns of convex cells: Voronoi cells around random sites, or planar slices and
//! bricks. Only arithmetic, `sqrt` and `libm` are used, so the cells are the same on every platform.

use concordia_core::rng::SimRng;
use manifold_sys::safe::Plane;
use serde::{Deserialize, Serialize};

use crate::{Bounds, FractureError};

/// The most cells a pattern may make.
pub const MAX_CELLS: u32 = 1024;

/// How a mesh is divided.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    /// Voronoi cells around `cells` sites spread evenly over the bounds.
    Uniform {
        /// The number of sites.
        cells: u32,
    },
    /// Voronoi cells around sites gathered near `clusters` random centres.
    Clustered {
        /// The number of centres.
        clusters: u32,
        /// Sites per centre.
        per_cluster: u32,
        /// How far sites stray from their centre, as a fraction of the bounds' size (0 to 1].
        spread: f32,
    },
    /// Voronoi cells around sites crowded towards an impact point: smaller pieces near it.
    Radial {
        /// Where the impact lands.
        impact: [f32; 3],
        /// The number of sites.
        cells: u32,
        /// How strongly sites crowd the impact (1 is even; higher crowds more).
        falloff: f32,
    },
    /// `count` equal slabs across the bounds, perpendicular to `normal`.
    Slices {
        /// The slicing direction (any length but zero).
        normal: [f32; 3],
        /// The number of slabs.
        count: u32,
    },
    /// A running bond of boxes aligned with the axes: every other row (along y) is offset by half
    /// a brick along x.
    Bricks {
        /// A brick's size.
        size: [f32; 3],
    },
}

/// A convex cell: where every plane's `normal · x ≤ offset` holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// The cell's site (its centre for slices and bricks).
    pub site: [f32; 3],
    /// Its bounding half-spaces.
    pub planes: Vec<Plane>,
}

impl Pattern {
    /// The cells for a mesh with these bounds.
    pub fn cells(&self, bounds: &Bounds, seed: u64) -> Result<Vec<Cell>, FractureError> {
        let mut rng = SimRng::new(seed);
        match *self {
            Self::Uniform { cells } => {
                count(cells)?;
                Ok(voronoi(uniform(bounds, cells, &mut rng)))
            }
            Self::Clustered {
                clusters,
                per_cluster,
                spread,
            } => {
                count(clusters.saturating_mul(per_cluster))?;
                if !(spread > 0.0 && spread <= 1.0) {
                    return invalid("`spread` must be in (0, 1]");
                }
                Ok(voronoi(clustered(
                    bounds,
                    clusters,
                    per_cluster,
                    spread,
                    &mut rng,
                )))
            }
            Self::Radial {
                impact,
                cells,
                falloff,
            } => {
                count(cells)?;
                if impact.iter().any(|v| !v.is_finite()) {
                    return invalid("`impact` must be finite");
                }
                if !(falloff.is_finite() && falloff > 0.0) {
                    return invalid("`falloff` must be a positive number");
                }
                Ok(voronoi(radial(bounds, impact, cells, falloff, &mut rng)))
            }
            Self::Slices { normal, count: n } => slices(bounds, normal, n),
            Self::Bricks { size } => bricks(bounds, size),
        }
    }
}

fn invalid<T>(why: &str) -> Result<T, FractureError> {
    Err(FractureError::InvalidPattern(why.into()))
}

fn count(n: u32) -> Result<(), FractureError> {
    match n {
        0 => invalid("a pattern needs at least one cell"),
        n if n > MAX_CELLS => invalid(&format!("{n} cells; at most {MAX_CELLS}")),
        _ => Ok(()),
    }
}

fn uniform(b: &Bounds, n: u32, rng: &mut SimRng) -> Vec<[f32; 3]> {
    (0..n).map(|_| point_in(b, rng)).collect()
}

fn point_in(b: &Bounds, rng: &mut SimRng) -> [f32; 3] {
    let s = b.size();
    [0, 1, 2].map(|a| b.min[a] + rng.next_f32() * s[a])
}

fn clustered(b: &Bounds, clusters: u32, per: u32, spread: f32, rng: &mut SimRng) -> Vec<[f32; 3]> {
    let s = b.size();
    let mut sites = Vec::new();
    for _ in 0..clusters {
        let c = point_in(b, rng);
        for _ in 0..per {
            sites.push([0, 1, 2].map(|a| c[a] + (rng.next_f32() * 2.0 - 1.0) * spread * s[a]));
        }
    }
    sites
}

fn radial(b: &Bounds, impact: [f32; 3], n: u32, falloff: f32, rng: &mut SimRng) -> Vec<[f32; 3]> {
    let r = b.radius();
    (0..n)
        .map(|_| {
            let d = direction(rng);
            let reach = r * libm::powf(rng.next_f32(), falloff);
            [0, 1, 2].map(|a| impact[a] + d[a] * reach)
        })
        .collect()
}

/// A random unit vector, by rejection from the cube (no trigonometry, so no platform drift).
fn direction(rng: &mut SimRng) -> [f32; 3] {
    loop {
        let v = [0; 3].map(|_| rng.next_f32() * 2.0 - 1.0);
        let l2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
        if l2 > 1e-4 && l2 <= 1.0 {
            let l = l2.sqrt();
            return v.map(|x| x / l);
        }
    }
}

/// One Voronoi cell per distinct site: the bisector planes against every other site.
fn voronoi(sites: Vec<[f32; 3]>) -> Vec<Cell> {
    let mut distinct: Vec<[f32; 3]> = Vec::with_capacity(sites.len());
    for s in sites {
        if !distinct.contains(&s) {
            distinct.push(s);
        }
    }
    distinct
        .iter()
        .map(|&a| Cell {
            site: a,
            planes: distinct
                .iter()
                .filter(|&&b| b != a)
                .map(|&b| bisector(a, b))
                .collect(),
        })
        .collect()
}

/// The half-space nearer `a` than `b`. Swapping them negates the plane exactly.
fn bisector(a: [f32; 3], b: [f32; 3]) -> Plane {
    let normal = [0, 1, 2].map(|i| b[i] - a[i]);
    let mid = [0, 1, 2].map(|i| (a[i] + b[i]) * 0.5);
    Plane {
        normal,
        offset: normal[0] * mid[0] + normal[1] * mid[1] + normal[2] * mid[2],
    }
}

/// The slab between `lo · n ≤ x · n ≤ hi`, open on a side whose bound is `None`.
fn slab(n: [f32; 3], lo: Option<f32>, hi: Option<f32>) -> Vec<Plane> {
    let mut planes = Vec::new();
    if let Some(lo) = lo {
        planes.push(Plane {
            normal: n.map(|v| -v),
            offset: -lo,
        });
    }
    if let Some(hi) = hi {
        planes.push(Plane {
            normal: n,
            offset: hi,
        });
    }
    planes
}

fn slices(b: &Bounds, normal: [f32; 3], n: u32) -> Result<Vec<Cell>, FractureError> {
    count(n)?;
    let l2 = normal.iter().map(|v| v * v).sum::<f32>();
    if !(l2.is_finite() && l2 > 1e-12) {
        return invalid("`normal` must be finite and not zero");
    }
    let unit = normal.map(|v| v / l2.sqrt());
    let corners =
        (0..8).map(|i| [0, 1, 2].map(|a| if i >> a & 1 == 0 { b.min[a] } else { b.max[a] }));
    let along: Vec<f32> = corners.map(|c| dot(unit, c)).collect();
    let lo = along.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = along.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let edge = |k: u32| lo + (hi - lo) * k as f32 / n as f32;
    Ok((0..n)
        .map(|k| {
            let mid = (edge(k) + edge(k + 1)) * 0.5;
            Cell {
                site: [0, 1, 2].map(|a| unit[a] * mid),
                planes: slab(
                    unit,
                    (k > 0).then(|| edge(k)),
                    (k + 1 < n).then(|| edge(k + 1)),
                ),
            }
        })
        .collect())
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn bricks(b: &Bounds, size: [f32; 3]) -> Result<Vec<Cell>, FractureError> {
    if size.iter().any(|s| !(s.is_finite() && *s > 0.0)) {
        return invalid("a brick's `size` must be positive on every axis");
    }
    let extent = b.size();
    // Rows and layers cover the bounds; offset rows need one brick more along x.
    let n = [0, 1, 2].map(|a| ((extent[a] / size[a]).ceil() as u64).max(1));
    let (nx, ny, nz) = (n[0] + 1, n[1], n[2]);
    if nx.saturating_mul(ny).saturating_mul(nz) > u64::from(MAX_CELLS) {
        return invalid(&format!(
            "bricks this small make more than {MAX_CELLS} cells"
        ));
    }
    let edge = |a: usize, i: u64, shift: f32| b.min[a] + i as f32 * size[a] - shift;
    let axis = |a: usize| {
        let mut n = [0.0; 3];
        n[a] = 1.0;
        n
    };
    let mut cells = Vec::new();
    for z in 0..nz {
        for y in 0..ny {
            let shift = if y % 2 == 1 { size[0] * 0.5 } else { 0.0 };
            for x in 0..nx {
                let lo = [edge(0, x, shift), edge(1, y, 0.0), edge(2, z, 0.0)];
                let hi = [
                    edge(0, x + 1, shift),
                    edge(1, y + 1, 0.0),
                    edge(2, z + 1, 0.0),
                ];
                cells.push(Cell {
                    site: [0, 1, 2].map(|a| (lo[a] + hi[a]) * 0.5),
                    planes: (0..3)
                        .flat_map(|a| slab(axis(a), Some(lo[a]), Some(hi[a])))
                        .collect(),
                });
            }
        }
    }
    Ok(cells)
}
