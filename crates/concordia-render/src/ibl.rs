//! Image-based lighting, computed on the CPU (deterministic and testable): equirectangular
//! environments, SH9 irradiance and GGX-prefiltered specular levels; the split-sum BRDF table
//! comes from `concordia-material` (re-exported here).
//!
//! References: Brian Karis, "Real Shading in Unreal Engine 4" (SIGGRAPH 2013 course notes; split
//! sum); Ramamoorthi & Hanrahan, "An Efficient Representation for Irradiance Environment Maps"
//! (SIGGRAPH 2001); Colbert & Křivánek, "GPU-Based Importance Sampling" (GPU Gems 3, ch. 20).

use std::f32::consts::{PI, TAU};

use concordia_assets::{TextureData, TextureFormat};
use concordia_core::math::Vec3;
pub use concordia_material::dfg::{
    BRDF_LUT_SAMPLES, BRDF_LUT_SIZE, brdf_lut, brdf_lut_cached, integrate_brdf,
};
use concordia_material::sampling::{hammersley, importance_ggx};

/// A latitude-longitude map of linear RGB radiance (row 0 is straight up).
#[derive(Debug, Clone, PartialEq)]
pub struct Equirect {
    width: usize,
    height: usize,
    pixels: Vec<[f32; 3]>,
}

impl Equirect {
    /// A map from rows of pixels; `pixels.len()` must be `width * height`, both non-zero.
    pub fn new(width: usize, height: usize, pixels: Vec<[f32; 3]>) -> Result<Self, String> {
        if width == 0 || height == 0 || pixels.len() != width * height {
            return Err(format!(
                "equirect {width}×{height} needs {} pixels, got {}",
                width * height,
                pixels.len()
            ));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Decodes mip 0 of an RGBA16F (HDR) or RGBA8 texture.
    pub fn from_texture(texture: &TextureData) -> Result<Self, String> {
        let bytes = texture.mips.first().ok_or("texture has no mips")?;
        let pixels: Vec<[f32; 3]> = match texture.format {
            TextureFormat::Rgba16Float => bytes
                .as_chunks::<8>()
                .0
                .iter()
                .map(|p| [0, 2, 4].map(|i| half::f16::from_le_bytes([p[i], p[i + 1]]).to_f32()))
                .collect(),
            TextureFormat::Rgba8Linear => bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| [p[0], p[1], p[2]].map(|c| c as f32 / 255.0))
                .collect(),
            TextureFormat::Rgba8Srgb => bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| [p[0], p[1], p[2]].map(srgb_to_linear))
                .collect(),
            TextureFormat::R8 => return Err("a one-channel texture can't light a scene".into()),
        };
        Self::new(texture.width as usize, texture.height as usize, pixels)
    }

    /// Width.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Height.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Pixels, rows from the top.
    pub fn pixels(&self) -> &[[f32; 3]] {
        &self.pixels
    }

    /// One pixel.
    pub fn get(&self, x: usize, y: usize) -> [f32; 3] {
        self.pixels[y * self.width + x]
    }

    /// Sets one pixel.
    pub fn set(&mut self, x: usize, y: usize, value: [f32; 3]) {
        self.pixels[y * self.width + x] = value;
    }

    /// Bilinear sample at a direction (u wraps, v clamps).
    pub fn sample(&self, dir: Vec3) -> [f32; 3] {
        let [u, v] = dir_to_uv(dir);
        let (x, y) = (
            u * self.width as f32 - 0.5,
            (v * self.height as f32 - 0.5).clamp(0.0, self.height as f32 - 1.0),
        );
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let wrap = |x: f32| (x as i64).rem_euclid(self.width as i64) as usize;
        let (xa, xb) = (wrap(x0), wrap(x0 + 1.0));
        let (ya, yb) = (y0 as usize, (y0 as usize + 1).min(self.height - 1));
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
        lerp(
            lerp(self.get(xa, ya), self.get(xb, ya), fx),
            lerp(self.get(xa, yb), self.get(xb, yb), fx),
            fy,
        )
    }

    /// Half the size by 2×2 averaging (at least 1×1).
    fn downsample(&self) -> Self {
        let (w, h) = ((self.width / 2).max(1), (self.height / 2).max(1));
        let mut pixels = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let taps = [
                    (2 * x, 2 * y),
                    (2 * x + 1, 2 * y),
                    (2 * x, 2 * y + 1),
                    (2 * x + 1, 2 * y + 1),
                ]
                .map(|(sx, sy)| self.get(sx.min(self.width - 1), sy.min(self.height - 1)));
                pixels.push([0, 1, 2].map(|i| taps.iter().map(|t| t[i]).sum::<f32>() / 4.0));
            }
        }
        Self {
            width: w,
            height: h,
            pixels,
        }
    }
}

fn srgb_to_linear(c: u8) -> f32 {
    let x = c as f32 / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

pub use concordia_render_types::equirect::{dir_to_uv, uv_to_dir};

/// The nine real SH basis functions at `n`.
fn sh_basis(n: Vec3) -> [f32; 9] {
    let (x, y, z) = (n.x, n.y, n.z);
    [
        0.282_095,
        0.488_603 * y,
        0.488_603 * z,
        0.488_603 * x,
        1.092_548 * x * y,
        1.092_548 * y * z,
        0.315_392 * (3.0 * z * z - 1.0),
        1.092_548 * x * z,
        0.546_274 * (x * x - y * y),
    ]
}

/// Irradiance as SH9 (radiance projected, then convolved with the clamped cosine).
pub fn sh9_irradiance(map: &Equirect) -> [[f32; 3]; 9] {
    let mut sh = [[0.0f32; 3]; 9];
    // Each row is split into sub-bands with exact solid angles; column sums are already exact
    // for the low-order terms, so this brings the quadrature error well below 1e-4.
    const SUB: usize = 8;
    for y in 0..map.height {
        for sub in 0..SUB {
            let band = |k: usize| (y * SUB + k) as f32 / (map.height * SUB) as f32;
            let (t0, t1) = (band(sub) * PI, band(sub + 1) * PI);
            let d_omega = (t0.cos() - t1.cos()) * TAU / map.width as f32;
            let v = (band(sub) + band(sub + 1)) / 2.0;
            for x in 0..map.width {
                let dir = uv_to_dir([(x as f32 + 0.5) / map.width as f32, v]);
                let radiance = map.get(x, y);
                for (coef, basis) in sh.iter_mut().zip(sh_basis(dir)) {
                    for c in 0..3 {
                        coef[c] += radiance[c] * basis * d_omega;
                    }
                }
            }
        }
    }
    const BAND: [f32; 9] = [
        PI,
        2.0 * PI / 3.0,
        2.0 * PI / 3.0,
        2.0 * PI / 3.0,
        PI / 4.0,
        PI / 4.0,
        PI / 4.0,
        PI / 4.0,
        PI / 4.0,
    ];
    for (coef, a) in sh.iter_mut().zip(BAND) {
        *coef = coef.map(|v| v * a);
    }
    sh
}

/// Irradiance at normal `n` from [`sh9_irradiance`] coefficients.
pub fn eval_sh9(sh: &[[f32; 3]; 9], n: Vec3) -> [f32; 3] {
    let basis = sh_basis(n.normalize_or(Vec3::Y));
    [0, 1, 2].map(|c| {
        sh.iter()
            .zip(basis)
            .map(|(coef, b)| coef[c] * b)
            .sum::<f32>()
            .max(0.0)
    })
}

/// Prefiltered specular levels for roughness 0, 1/(levels-1), …, 1; level 0 is the source and
/// level i ≥ 1 is the source shrunk by 2^(i+1) (at least 32×16; rough levels are blurry anyway).
pub fn prefilter(map: &Equirect, levels: usize, samples: u32) -> Vec<Equirect> {
    let mut mips = vec![map.clone()];
    while mips.last().is_some_and(|m| m.width > 1 && m.height > 1) {
        let next = mips[mips.len() - 1].downsample();
        mips.push(next);
    }
    let texel_solid_angle = 4.0 * PI / (map.width * map.height) as f32;
    let mut out = vec![map.clone()];
    for level in 1..levels {
        let roughness = level as f32 / (levels - 1).max(1) as f32;
        let alpha = roughness * roughness;
        let (w, h) = (
            (map.width >> (level + 1)).max(32).min(map.width),
            (map.height >> (level + 1)).max(16).min(map.height),
        );
        let mut pixels = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let n = uv_to_dir([(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32]);
                pixels.push(prefilter_texel(&mips, n, alpha, samples, texel_solid_angle));
            }
        }
        out.push(Equirect {
            width: w,
            height: h,
            pixels,
        });
    }
    out
}

/// GGX-weighted average of incoming radiance around `n` (N = V = R), sampling coarser mips where
/// the sample's footprint is larger than a texel (filtered importance sampling).
fn prefilter_texel(
    mips: &[Equirect],
    n: Vec3,
    alpha: f32,
    samples: u32,
    texel_solid_angle: f32,
) -> [f32; 3] {
    let (mut sum, mut weight) = ([0.0f32; 3], 0.0f32);
    for i in 0..samples {
        let h = importance_ggx(hammersley(i, samples), n, alpha);
        let l = 2.0 * n.dot(h) * h - n;
        let nol = n.dot(l);
        if nol <= 0.0 {
            continue;
        }
        let noh = n.dot(h).max(1e-4);
        let a2 = alpha * alpha;
        let d = a2 / (PI * (noh * noh * (a2 - 1.0) + 1.0).powi(2)).max(1e-8);
        let pdf = d / 4.0;
        let sample_solid_angle = 1.0 / (samples as f32 * pdf).max(1e-8);
        let lod = (0.5 * (sample_solid_angle / texel_solid_angle).log2()).max(0.0);
        let mip = (lod.round() as usize).min(mips.len() - 1);
        let radiance = mips[mip].sample(l);
        for c in 0..3 {
            sum[c] += radiance[c] * nol;
        }
        weight += nol;
    }
    sum.map(|s| s / weight.max(1e-8))
}

/// RGBA16F texture data of an equirect (alpha 1).
pub(crate) fn to_texture(map: &Equirect) -> TextureData {
    let bytes = map
        .pixels
        .iter()
        .flat_map(|p| [p[0], p[1], p[2], 1.0])
        .flat_map(|c| half::f16::from_f32(c).to_le_bytes())
        .collect();
    TextureData {
        width: map.width as u32,
        height: map.height as u32,
        format: TextureFormat::Rgba16Float,
        mips: vec![bytes],
    }
}

/// The albedo tables as one RGBA16F texture, two tables wide: at each (N·V, roughness) texel the
/// left half holds the split sum (A, B) and the direct lobe's (A, B), the right half the sheen
/// albedo (E, 0, 0, 1). The shader reads each half at texel centers, as the CPU tables do.
pub fn albedo_table() -> TextureData {
    let size = BRDF_LUT_SIZE;
    let (split, direct) = (
        brdf_lut_cached(),
        concordia_material::dfg::direct_lut_cached(),
    );
    let sheen = concordia_material::dfg::sheen_lut_cached();
    let texel = |x: usize, y: usize| match x < size {
        true => {
            let i = y * size + x;
            [split[i][0], split[i][1], direct[i][0], direct[i][1]]
        }
        false => [sheen[y * size + x - size], 0.0, 0.0, 1.0],
    };
    let bytes = (0..size)
        .flat_map(|y| (0..2 * size).map(move |x| (x, y)))
        .flat_map(|(x, y)| texel(x, y))
        .flat_map(|c| half::f16::from_f32(c).to_le_bytes())
        .collect();
    TextureData {
        width: 2 * size as u32,
        height: size as u32,
        format: TextureFormat::Rgba16Float,
        mips: vec![bytes],
    }
}

/// Specular levels the renderer prefilters (roughness 0, 0.2, …, 1).
pub const SPECULAR_LEVELS: usize = 6;
/// GGX samples per prefiltered texel.
pub const PREFILTER_SAMPLES: u32 = 64;
