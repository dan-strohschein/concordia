//! Tileable textures on the CPU: value and Worley noise (fractal octaves) and patterns, mapped
//! between two colours, as RGBA8 PNG. Noise lattices wrap at the image's edge, so the texture
//! tiles without a seam.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::GenError;
use crate::provider::{ContentProvider, GenKind, Generated};

/// A square texture: a pattern between two colours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TextureRequest {
    /// Width and height in pixels (a power of two, 4–4096).
    pub size: u32,
    /// What to draw.
    pub pattern: Pattern,
    /// The colours at 0 and 1 (RGBA8).
    pub colors: [[u8; 4]; 2],
}

/// What a texture shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Pattern {
    /// Tileable fractal noise.
    Noise {
        /// Value (soft blobs) or Worley (cells).
        kind: NoiseKind,
        /// Lattice cells across the image at the first octave (divides the size).
        cells: u32,
        /// Octaves, each twice as fine and half as strong (1–8).
        octaves: u32,
    },
    /// A checkerboard of `cells` × `cells` squares (cells divides the size).
    Checker {
        /// Squares across.
        cells: u32,
    },
    /// Vertical stripes, alternating colours (count divides the size).
    Stripes {
        /// Stripes across.
        count: u32,
    },
}

/// A noise basis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum NoiseKind {
    /// Smoothly interpolated lattice values.
    Value,
    /// Distance to the nearest of one seeded point per cell.
    Worley,
}

fn divides(field: &str, n: u32, size: u32) -> Result<(), GenError> {
    if n == 0 || !size.is_multiple_of(n) {
        return Err(GenError::invalid(
            field,
            format!("{n} must divide the size {size}"),
        ));
    }
    Ok(())
}

fn check(r: &TextureRequest) -> Result<(), GenError> {
    if !(r.size.is_power_of_two() && (4..=4096).contains(&r.size)) {
        return Err(GenError::invalid(
            "size",
            format!("{} is not a power of two in 4–4096", r.size),
        ));
    }
    match &r.pattern {
        Pattern::Noise { cells, octaves, .. } => {
            divides("cells", *cells, r.size)?;
            if !(1..=8).contains(octaves) {
                return Err(GenError::invalid(
                    "octaves",
                    format!("{octaves} is outside 1–8"),
                ));
            }
        }
        Pattern::Checker { cells } => divides("cells", *cells, r.size)?,
        Pattern::Stripes { count } => divides("count", *count, r.size)?,
    }
    Ok(())
}

/// A 32-bit hash of a wrapped lattice point.
fn hash(x: i64, y: i64, period: i64, seed: u64) -> u32 {
    let (x, y) = (x.rem_euclid(period) as u32, y.rem_euclid(period) as u32);
    let mut h = (seed as u32) ^ (seed >> 32) as u32 ^ period as u32;
    for v in [x, y] {
        h ^= v.wrapping_mul(0x9E37_79B1);
        h = (h ^ (h >> 15)).wrapping_mul(0x85EB_CA77);
        h = (h ^ (h >> 13)).wrapping_mul(0xC2B2_AE3D);
        h ^= h >> 16;
    }
    h
}

fn unit(h: u32) -> f32 {
    h as f32 / u32::MAX as f32
}

/// Value noise in [0, 1] at (u, v) in lattice units, wrapping every `period` cells.
fn value(u: f32, v: f32, period: i64, seed: u64) -> f32 {
    let (x0, y0) = (u.floor(), v.floor());
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(u - x0), s(v - y0));
    let (ix, iy) = (x0 as i64, y0 as i64);
    let l = |dx: i64, dy: i64| unit(hash(ix + dx, iy + dy, period, seed));
    let top = l(0, 0) * (1.0 - sx) + l(1, 0) * sx;
    let bottom = l(0, 1) * (1.0 - sx) + l(1, 1) * sx;
    top * (1.0 - sy) + bottom * sy
}

/// Worley (F1) noise in [0, 1]: distance to the nearest cell point, wrapping every `period`.
fn worley(u: f32, v: f32, period: i64, seed: u64) -> f32 {
    let (cx, cy) = (u.floor() as i64, v.floor() as i64);
    let mut best = f32::MAX;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (x, y) = (cx + dx, cy + dy);
            let h = hash(x, y, period, seed);
            let (px, py) = (x as f32 + unit(h), y as f32 + unit(h.rotate_left(16)));
            best = best.min(((px - u).powi(2) + (py - v).powi(2)).sqrt());
        }
    }
    (best / std::f32::consts::SQRT_2).min(1.0)
}

/// The pattern's value in [0, 1] at pixel (x, y).
fn sample(r: &TextureRequest, x: u32, y: u32, seed: u64) -> f32 {
    match &r.pattern {
        Pattern::Noise {
            kind,
            cells,
            octaves,
        } => {
            let (mut sum, mut weight, mut amplitude) = (0.0, 0.0, 1.0);
            for o in 0..*octaves {
                let period = i64::from(cells << o);
                let scale = period as f32 / r.size as f32;
                let (u, v) = ((x as f32 + 0.5) * scale, (y as f32 + 0.5) * scale);
                let n = match kind {
                    NoiseKind::Value => value(u, v, period, seed.wrapping_add(u64::from(o))),
                    NoiseKind::Worley => worley(u, v, period, seed.wrapping_add(u64::from(o))),
                };
                sum += n * amplitude;
                weight += amplitude;
                amplitude *= 0.5;
            }
            sum / weight
        }
        Pattern::Checker { cells } => {
            let cell = r.size / cells;
            ((x / cell + y / cell) % 2) as f32
        }
        Pattern::Stripes { count } => ((x / (r.size / count)) % 2) as f32,
    }
}

/// RGBA8 pixels of `r`, row by row.
pub(crate) fn render(r: &TextureRequest, seed: u64) -> Result<Vec<u8>, GenError> {
    check(r)?;
    let [a, b] = r.colors;
    let mut out = Vec::with_capacity((r.size * r.size * 4) as usize);
    for y in 0..r.size {
        for x in 0..r.size {
            let t = sample(r, x, y, seed).clamp(0.0, 1.0);
            out.extend((0..4).map(|i| {
                (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t).round() as u8
            }));
        }
    }
    Ok(out)
}

fn png(size: u32, rgba: &[u8]) -> Result<Vec<u8>, GenError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| GenError::Output(e.to_string()))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| GenError::Output(e.to_string()))?;
    writer
        .finish()
        .map_err(|e| GenError::Output(e.to_string()))?;
    Ok(out)
}

/// The `texture` provider: a [`TextureRequest`] as a `.png`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextureProvider;

impl ContentProvider for TextureProvider {
    fn id(&self) -> &str {
        "texture"
    }
    fn kind(&self) -> GenKind {
        GenKind::texture
    }
    fn version(&self) -> u32 {
        1
    }
    fn extensions(&self) -> Vec<&str> {
        vec!["png"]
    }
    fn request_schema(&self) -> Value {
        concordia_reflect::schema_for(<TextureRequest as Reflect>::type_info())
    }
    fn generate(
        &self,
        request: &Value,
        seed: u64,
        _: &crate::provider::GenContext<'_>,
    ) -> Result<Generated, GenError> {
        let r: TextureRequest = serde_json::from_value(request.clone())
            .map_err(|e| GenError::invalid("request", e.to_string()))?;
        let rgba = render(&r, seed)?;
        Ok(Generated {
            bytes: png(r.size, &rgba)?,
            info: json!({"width": r.size, "height": r.size}),
            settings: None,
            inputs: Vec::new(),
        })
    }
}
