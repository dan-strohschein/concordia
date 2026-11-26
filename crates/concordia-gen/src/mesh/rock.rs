//! Seeded rocks: a few lumps (ellipsoids and rounded boxes) smooth-unioned, displaced by 3D
//! fractal noise, with optional ledges and a flat base, meshed by surface nets. The noise is
//! evaluated only near the lumps' surface (a narrow band): elsewhere it cannot change the sign.

use concordia_assets::MeshData;

use super::noise::fbm3;
use super::sdf::{Grid, surface_nets};
use super::sweep::positive;
use crate::error::GenError;
use crate::rng::Rng;

/// Noise octaves on a rock, at most: finer ones are dropped until each spans 4 cells.
const OCTAVES: u32 = 4;
/// The finest noise wavelength and the narrowest ledge lip, in cells (finer features would be
/// thinner than the grid and tear the surface).
const FINEST_CELLS: f32 = 4.0;
const LIP_CELLS: f32 = 2.5;
/// Band half-width, in cells, past the noise's reach.
const BAND_CELLS: f32 = 3.0;

/// A rock's parameters (see `MeshRequest::Rock`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RockShape {
    pub size: [f32; 3],
    pub detail: u32,
    pub roughness: f32,
    pub strata: f32,
    pub lumps: u32,
}

/// One lump: an ellipsoid or a rounded box.
#[derive(Debug, Clone, Copy)]
struct Lump {
    centre: [f32; 3],
    radii: [f32; 3],
    boxy: bool,
}

/// A rock's distance field, built once from its shape and seed.
struct Field {
    lumps: Vec<Lump>,
    blend: f32,
    amplitude: f32,
    frequency: f32,
    offset: [f32; 3],
    strata: f32,
    ledge: f32,
    lip: f32,
    octaves: u32,
    phase: f32,
    base: f32,
    band: f32,
    seed: u64,
}

/// The rock `shape` for `seed`.
pub(crate) fn rock(shape: &RockShape, seed: u64) -> Result<MeshData, GenError> {
    mesh(shape, seed, true)
}

/// The rock, its noise evaluated in the narrow band only (`band`) or everywhere.
fn mesh(shape: &RockShape, seed: u64, band: bool) -> Result<MeshData, GenError> {
    validate(shape)?;
    let half = shape.size.map(|s| s * 0.5);
    let small = half[0].min(half[1]).min(half[2]);
    let longest = shape.size[0].max(shape.size[1]).max(shape.size[2]);
    let amplitude = shape.roughness * small;
    let ledge = shape.strata * 0.15;
    let reach = amplitude + ledge + 0.1 * small;
    let cell = (longest + 2.0 * reach) / (shape.detail as f32 - 2.0 * BAND_CELLS);
    let grid = Grid::around(half.map(|h| -h), half, reach + 2.0 * cell, cell)?;
    let mut field = Field::new(shape, seed, amplitude, ledge, half);
    field.octaves = octaves(field.frequency, cell);
    if shape.strata > 0.0 {
        field.lip = (LIP_CELLS * cell / shape.strata).clamp(0.15, 0.5);
    }
    field.band = match band {
        true => amplitude + ledge + BAND_CELLS * cell,
        false => f32::INFINITY,
    };
    Ok(surface_nets(&|p| field.distance(p), &grid))
}

/// Octaves of noise at `frequency` whose wavelength spans at least `FINEST_CELLS` cells
/// (1–`OCTAVES`).
fn octaves(frequency: f32, cell: f32) -> u32 {
    let mut wavelength = 1.0 / frequency;
    let mut n = 0;
    while n < OCTAVES && wavelength >= FINEST_CELLS * cell {
        n += 1;
        wavelength *= 0.5;
    }
    n.max(1)
}

fn validate(s: &RockShape) -> Result<(), GenError> {
    s.size.iter().try_for_each(|v| positive("size", *v))?;
    if !(8..=250).contains(&s.detail) {
        return Err(GenError::invalid("detail", "must be 8–250 cells"));
    }
    if !(s.roughness.is_finite() && (0.0..=0.5).contains(&s.roughness)) {
        return Err(GenError::invalid("roughness", "must be 0–0.5"));
    }
    if !(s.strata.is_finite() && s.strata >= 0.0) {
        return Err(GenError::invalid("strata", "must be ≥ 0"));
    }
    if !(1..=8).contains(&s.lumps) {
        return Err(GenError::invalid("lumps", "must be 1–8"));
    }
    Ok(())
}

impl Field {
    fn new(shape: &RockShape, seed: u64, amplitude: f32, ledge: f32, half: [f32; 3]) -> Self {
        let mut rng = Rng::new(seed);
        let lumps = (0..shape.lumps)
            .map(|i| match i {
                0 => Lump {
                    centre: [0.0; 3],
                    radii: half.map(|h| h * rng.range(0.78, 0.86)),
                    boxy: rng.unit() < 0.5,
                },
                _ => Lump {
                    centre: half.map(|h| h * rng.range(-0.32, 0.32)),
                    radii: half.map(|h| h * rng.range(0.38, 0.6)),
                    boxy: rng.unit() < 0.5,
                },
            })
            .collect();
        let small = half[0].min(half[1]).min(half[2]);
        Self {
            lumps,
            blend: 0.3 * small,
            amplitude,
            frequency: 2.5 / small,
            offset: [0; 3].map(|_| rng.range(0.0, 1000.0)),
            strata: shape.strata,
            ledge,
            lip: 0.15,
            octaves: OCTAVES,
            phase: rng.range(0.0, shape.strata.max(1.0)),
            base: -half[1] * 0.7,
            band: f32::INFINITY,
            seed,
        }
    }

    /// The signed distance at `p` (negative inside).
    fn distance(&self, p: [f32; 3]) -> f32 {
        let lumps = self.lumps(p);
        let cut = self.base - p[1];
        if lumps.abs() > self.band {
            return lumps.max(cut);
        }
        (lumps + self.detail(p)).max(cut)
    }

    /// The lumps' smooth union.
    fn lumps(&self, p: [f32; 3]) -> f32 {
        self.lumps
            .iter()
            .map(|l| l.distance(p))
            .reduce(|a, b| super::sdf::smooth_min(a, b, self.blend))
            .unwrap_or(f32::MAX)
    }

    /// The noise and the ledges at `p` (in −amplitude ..= amplitude + ledge).
    fn detail(&self, p: [f32; 3]) -> f32 {
        let q: [f32; 3] = std::array::from_fn(|i| (p[i] + self.offset[i]) * self.frequency);
        let noise = self.amplitude * fbm3(q, self.seed, self.octaves);
        if self.strata <= 0.0 {
            return noise;
        }
        let t = (p[1] + self.phase) / self.strata;
        let f = t - t.floor();
        let rise = 1.0 - self.lip;
        let lip = match f < rise {
            true => {
                let u = f / rise;
                u * u * (3.0 - 2.0 * u)
            }
            false => (1.0 - f) / self.lip,
        };
        noise + self.ledge * lip
    }
}

impl Lump {
    fn distance(&self, p: [f32; 3]) -> f32 {
        let d: [f32; 3] = std::array::from_fn(|i| p[i] - self.centre[i]);
        match self.boxy {
            true => rounded_box(d, self.radii),
            false => ellipsoid(d, self.radii),
        }
    }
}

/// Quílez's ellipsoid bound: k0 (k0 − 1) / k1 with k0 = |p / r| and k1 = |p / r²|.
fn ellipsoid(p: [f32; 3], r: [f32; 3]) -> f32 {
    let len = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let k0 = len(std::array::from_fn(|i| p[i] / r[i]));
    let k1 = len(std::array::from_fn(|i| p[i] / (r[i] * r[i])));
    if k1 < 1e-9 {
        return -r[0].min(r[1]).min(r[2]);
    }
    k0 * (k0 - 1.0) / k1
}

/// A box of half extents `r` with its edges rounded by a third of the smallest.
fn rounded_box(p: [f32; 3], r: [f32; 3]) -> f32 {
    let round = r[0].min(r[1]).min(r[2]) / 3.0;
    let q: [f32; 3] = std::array::from_fn(|i| p[i].abs() - (r[i] - round));
    let out = q.map(|c| c.max(0.0));
    (out[0] * out[0] + out[1] * out[1] + out[2] * out[2]).sqrt() + q[0].max(q[1]).max(q[2]).min(0.0)
        - round
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Evaluating the noise only in the narrow band gives exactly the mesh of evaluating it
    /// everywhere: outside the band it can't change the field's sign.
    #[test]
    fn narrow_band_mesh_equals_full_grid_mesh() {
        for (seed, roughness, strata) in [(1, 0.25, 0.4), (2, 0.5, 0.0), (3, 0.1, 1.0)] {
            let shape = RockShape {
                size: [3.0, 2.0, 2.5],
                detail: 56,
                roughness,
                strata,
                lumps: 5,
            };
            let banded = mesh(&shape, seed, true).unwrap();
            let full = mesh(&shape, seed, false).unwrap();
            assert!(!banded.indices.is_empty());
            assert_eq!(banded.indices, full.indices, "seed {seed}");
            assert_eq!(banded.positions, full.positions, "seed {seed}");
            assert_eq!(banded.normals, full.normals, "seed {seed}");
        }
    }
}
