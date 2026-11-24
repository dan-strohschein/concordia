//! The sky: an equirectangular radiance map (the renderer's mapping), importance-sampled by
//! luminance × sin θ, as piecewise-constant texels.

use std::f32::consts::PI;

use concordia_core::math::Vec3;
use concordia_render_types::equirect::{dir_to_uv, uv_to_dir};

use crate::PathTraceError;

/// An equirectangular sky (radiance already scaled by its intensity).
#[derive(Debug, Clone, PartialEq)]
pub struct EnvMap {
    width: u32,
    height: u32,
    texels: Vec<Vec3>,
    /// Cumulative row weights (marginal), `height + 1` entries from 0 to 1.
    rows: Vec<f32>,
    /// Cumulative texel weights per row (conditional), `height × (width + 1)`.
    columns: Vec<f32>,
}

/// A direction drawn from the sky.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvSample {
    /// Toward the sky.
    pub dir: Vec3,
    /// Radiance from there.
    pub radiance: Vec3,
    /// Probability density per solid angle.
    pub pdf: f32,
}

impl EnvMap {
    /// A sky of `width × height` linear RGB texels, scaled by `intensity`.
    pub fn new(
        width: u32,
        height: u32,
        texels: &[[f32; 3]],
        intensity: f32,
    ) -> Result<Self, PathTraceError> {
        let bad = |m: String| Err(PathTraceError::Environment(m));
        if width == 0 || height == 0 || texels.len() != (width * height) as usize {
            return bad(format!(
                "{width}×{height} needs {} texels, got {}",
                width * height,
                texels.len()
            ));
        }
        if !(intensity.is_finite() && intensity >= 0.0) {
            return bad(format!("intensity {intensity}"));
        }
        if texels
            .iter()
            .flatten()
            .any(|c| !(c.is_finite() && *c >= 0.0))
        {
            return bad("texels must be finite and non-negative".into());
        }
        let texels: Vec<Vec3> = texels
            .iter()
            .map(|t| Vec3::from_array(*t) * intensity)
            .collect();
        let (rows, columns) = distributions(width, height, &texels);
        Ok(Self {
            width,
            height,
            texels,
            rows,
            columns,
        })
    }

    /// A uniform sky of `color` × `intensity`.
    pub fn uniform(color: [f32; 3], intensity: f32) -> Self {
        Self::new(1, 1, &[color], intensity).unwrap_or_else(|_| {
            let (rows, columns) = distributions(1, 1, &[Vec3::ZERO]);
            Self {
                width: 1,
                height: 1,
                texels: vec![Vec3::ZERO],
                rows,
                columns,
            }
        })
    }

    /// Radiance arriving from direction `dir`.
    pub fn radiance(&self, dir: Vec3) -> Vec3 {
        let (x, y) = self.texel(dir_to_uv(dir));
        self.texels[(y * self.width + x) as usize]
    }

    /// The density of [`EnvMap::sample`] drawing `dir`, per solid angle.
    pub fn pdf(&self, dir: Vec3) -> f32 {
        let (x, y) = self.texel(dir_to_uv(dir));
        self.texel_pdf(x, y)
    }

    /// Draws a direction with probability ∝ luminance × solid angle, from two uniform numbers.
    pub fn sample(&self, u: f32, v: f32) -> EnvSample {
        let y = pick(&self.rows, v);
        let w = self.width as usize + 1;
        let row = &self.columns[y * w..(y + 1) * w];
        let x = pick(row, u);
        // Uniform in solid angle inside the chosen texel (uniform in φ and in cos θ), where its
        // density is constant.
        let (fu, fv) = (frac(u, row, x), frac(v, &self.rows, y));
        let (c0, c1) = band_cosines(y as u32, self.height);
        let theta = (c0 - fv * (c0 - c1)).clamp(-1.0, 1.0).acos();
        let uv = [(x as f32 + fu) / self.width as f32, theta / PI];
        let dir = uv_to_dir(uv);
        EnvSample {
            dir,
            radiance: self.texels[y * self.width as usize + x],
            pdf: self.texel_pdf(x as u32, y as u32),
        }
    }

    fn texel(&self, [u, v]: [f32; 2]) -> (u32, u32) {
        let x = ((u.rem_euclid(1.0) * self.width as f32) as u32).min(self.width - 1);
        let y = ((v.clamp(0.0, 1.0) * self.height as f32) as u32).min(self.height - 1);
        (x, y)
    }

    /// Density per solid angle of texel (x, y): (its probability) / (its solid angle).
    fn texel_pdf(&self, x: u32, y: u32) -> f32 {
        let w = self.width as usize + 1;
        let (x, y) = (x as usize, y as usize);
        let p_row = self.rows[y + 1] - self.rows[y];
        let p_col = self.columns[y * w + x + 1] - self.columns[y * w + x];
        let (c0, c1) = band_cosines(y as u32, self.height);
        let solid = (c0 - c1) * 2.0 * PI / self.width as f32;
        if solid <= 0.0 {
            0.0
        } else {
            p_row * p_col / solid
        }
    }
}

/// cos θ at the top and bottom of texel row `y`.
fn band_cosines(y: u32, height: u32) -> (f32, f32) {
    let t = |r: u32| (r as f32 / height as f32 * PI).cos();
    (t(y), t(y + 1))
}

/// Marginal (row) and conditional (texel within row) cumulative weights; weight = luminance ×
/// the texel's solid angle. A black sky falls back to solid angle alone.
fn distributions(width: u32, height: u32, texels: &[Vec3]) -> (Vec<f32>, Vec<f32>) {
    let (w, h) = (width as usize, height as usize);
    let band = |y: usize| {
        let (c0, c1) = band_cosines(y as u32, height);
        (c0 - c1).max(0.0)
    };
    let lum = |t: Vec3| t.dot(Vec3::new(0.2126, 0.7152, 0.0722)).max(0.0);
    let dark = texels.iter().all(|t| lum(*t) == 0.0);
    let weight = |x: usize, y: usize| {
        if dark {
            band(y)
        } else {
            lum(texels[y * w + x]) * band(y)
        }
    };
    let mut columns = Vec::with_capacity(h * (w + 1));
    let mut row_sums = Vec::with_capacity(h);
    for y in 0..h {
        let mut acc = vec![0.0f32; w + 1];
        for x in 0..w {
            acc[x + 1] = acc[x] + weight(x, y);
        }
        let total = acc[w];
        row_sums.push(total);
        columns.extend(acc.iter().enumerate().map(|(i, a)| {
            if total > 0.0 {
                a / total
            } else {
                i as f32 / w as f32
            }
        }));
    }
    let mut rows = vec![0.0f32; h + 1];
    for y in 0..h {
        rows[y + 1] = rows[y] + row_sums[y];
    }
    let total = rows[h];
    let rows = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            if total > 0.0 {
                r / total
            } else {
                i as f32 / h as f32
            }
        })
        .collect();
    (rows, columns)
}

/// The bin of `u` in a cumulative list (first bin whose upper edge exceeds it, skipping empty bins).
fn pick(cdf: &[f32], u: f32) -> usize {
    let n = cdf.len() - 1;
    let i = cdf
        .partition_point(|c| *c <= u)
        .saturating_sub(1)
        .min(n - 1);
    (i..n).find(|&k| cdf[k + 1] > cdf[k]).unwrap_or(i)
}

/// Where `u` falls inside bin `i`, in [0, 1).
fn frac(u: f32, cdf: &[f32], i: usize) -> f32 {
    let span = cdf[i + 1] - cdf[i];
    if span > 0.0 {
        ((u - cdf[i]) / span).clamp(0.0, 0.999_999)
    } else {
        0.5
    }
}
