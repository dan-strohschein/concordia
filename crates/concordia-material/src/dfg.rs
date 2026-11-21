//! Directional-albedo tables: the split-sum specular table (A, B) and the sheen albedo E, each a
//! function of (N·V, perceptual roughness), computed once per process.
//!
//! References: Brian Karis, "Real Shading in Unreal Engine 4" (SIGGRAPH 2013 course notes; split
//! sum); Estevez & Kulla, "Production Friendly Microfacet Sheen BRDF" (SIGGRAPH 2017; albedo scaling).

use concordia_core::math::Vec3;

use crate::bsdf::{alpha, sheen_lobe, v_smith};
use crate::sampling::{hammersley, importance_ggx};

/// Split-sum scale and bias (A, B) for (N·V, roughness): specular ≈ prefiltered · (F0·A + B).
pub fn integrate_brdf(nov: f32, roughness: f32, samples: u32) -> [f32; 2] {
    let nov = nov.clamp(1e-4, 1.0);
    let v = Vec3::new((1.0 - nov * nov).sqrt(), 0.0, nov);
    let alpha = roughness * roughness;
    let k = alpha / 2.0;
    let g1 = |x: f32| x / (x * (1.0 - k) + k);
    let (mut a, mut b) = (0.0f32, 0.0f32);
    for i in 0..samples {
        let h = importance_ggx(hammersley(i, samples), Vec3::Z, alpha);
        let l = 2.0 * v.dot(h) * h - v;
        let (nol, noh, voh) = (l.z.max(0.0), h.z.max(0.0), v.dot(h).max(0.0));
        if nol > 0.0 {
            let g_vis = g1(nov) * g1(nol) * voh / (noh * nov).max(1e-8);
            let fc = (1.0 - voh).powi(5);
            a += (1.0 - fc) * g_vis;
            b += fc * g_vis;
        }
    }
    [a / samples as f32, b / samples as f32]
}

/// Side of the albedo tables.
pub const BRDF_LUT_SIZE: usize = 32;
/// Samples per entry (worst error vs a 65 536-sample reference ≈ 4e-4).
pub const BRDF_LUT_SAMPLES: u32 = 4096;

/// A `size`×`size` table of [`integrate_brdf`], rows by roughness, columns by N·V (texel centers).
pub fn brdf_lut(size: usize, samples: u32) -> Vec<[f32; 2]> {
    (0..size * size)
        .map(|i| {
            let (nov, rough) = ((i % size) as f32 + 0.5, (i / size) as f32 + 0.5);
            integrate_brdf(nov / size as f32, rough / size as f32, samples)
        })
        .collect()
}

/// The renderer's BRDF table, computed once per process.
pub fn brdf_lut_cached() -> &'static [[f32; 2]] {
    static LUT: std::sync::OnceLock<Vec<[f32; 2]>> = std::sync::OnceLock::new();
    LUT.get_or_init(|| brdf_lut(BRDF_LUT_SIZE, BRDF_LUT_SAMPLES))
}

/// The split-sum (A, B) at (N·V, roughness), read bilinearly from the cached table.
pub fn specular_albedo(nov: f32, roughness: f32) -> [f32; 2] {
    let lut = brdf_lut_cached();
    let ab = bilinear(BRDF_LUT_SIZE, nov, roughness, |i| {
        Vec3::new(lut[i][0], lut[i][1], 0.0)
    });
    [ab.x, ab.y]
}

/// Directional albedo (A, B) of the direct-light specular lobe (GGX, height-correlated Smith):
/// E = F0·A + B, read bilinearly from the cached table.
pub fn direct_albedo(nov: f32, roughness: f32) -> [f32; 2] {
    let lut = direct_lut_cached();
    let ab = bilinear(BRDF_LUT_SIZE, nov, roughness, |i| {
        Vec3::new(lut[i][0], lut[i][1], 0.0)
    });
    [ab.x, ab.y]
}

/// (A, B) of the direct-light lobe at (N·V, roughness) by GGX importance sampling.
pub fn integrate_direct(nov: f32, roughness: f32, samples: u32) -> [f32; 2] {
    let nov = clean(nov).max(1e-4);
    let v = Vec3::new((1.0 - nov * nov).sqrt(), 0.0, nov);
    let a = alpha(clean(roughness));
    let samples = samples.max(1);
    let (mut sa, mut sb) = (0.0f32, 0.0f32);
    for i in 0..samples {
        let h = importance_ggx(hammersley(i, samples), Vec3::Z, a);
        let l = 2.0 * v.dot(h) * h - v;
        let (nol, noh, voh) = (l.z, h.z.max(1e-4), v.dot(h).max(0.0));
        if nol > 0.0 {
            let w = 4.0 * v_smith(nov + 1e-5, nol, a) * nol * voh / noh;
            let fc = (1.0 - voh).powi(5);
            sa += (1.0 - fc) * w;
            sb += fc * w;
        }
    }
    [sa / samples as f32, sb / samples as f32]
}

/// The direct-lobe albedo table, computed once per process.
pub fn direct_lut_cached() -> &'static [[f32; 2]] {
    static LUT: std::sync::OnceLock<Vec<[f32; 2]>> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        let size = BRDF_LUT_SIZE;
        (0..size * size)
            .map(|i| {
                let (nov, rough) = ((i % size) as f32 + 0.5, (i / size) as f32 + 0.5);
                integrate_direct(nov / size as f32, rough / size as f32, BRDF_LUT_SAMPLES)
            })
            .collect()
    })
}

/// Directional albedo of the sheen lobe at (N·V, roughness), by brute-force integration
/// (uniform hemisphere sampling).
pub fn integrate_sheen(nov: f32, roughness: f32, samples: u32) -> f32 {
    let nov = clean(nov).max(1e-4);
    let v = Vec3::new((1.0 - nov * nov).sqrt(), 0.0, nov);
    let samples = samples.max(1);
    let mut sum = 0.0f32;
    for i in 0..samples {
        let (u, w) = hammersley(i, samples);
        let (z, phi) = (w, std::f32::consts::TAU * u);
        let r = (1.0 - z * z).max(0.0).sqrt();
        let l = Vec3::new(r * phi.cos(), r * phi.sin(), z);
        let h = (v + l).normalize_or_zero();
        sum += sheen_lobe(nov, l.z, h.z.max(0.0), clean(roughness));
    }
    sum * std::f32::consts::TAU / samples as f32
}

/// A `size`×`size` table of [`integrate_sheen`], rows by roughness, columns by N·V.
pub fn sheen_lut(size: usize, samples: u32) -> Vec<f32> {
    (0..size * size)
        .map(|i| {
            let (nov, rough) = ((i % size) as f32 + 0.5, (i / size) as f32 + 0.5);
            integrate_sheen(nov / size as f32, rough / size as f32, samples)
        })
        .collect()
}

/// The sheen albedo table, computed once per process.
pub fn sheen_lut_cached() -> &'static [f32] {
    static LUT: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    LUT.get_or_init(|| sheen_lut(BRDF_LUT_SIZE, BRDF_LUT_SAMPLES))
}

/// Directional albedo of the sheen lobe at (N·V, roughness), read bilinearly from the cached table.
pub fn sheen_albedo(nov: f32, roughness: f32) -> f32 {
    let lut = sheen_lut_cached();
    bilinear(BRDF_LUT_SIZE, nov, roughness, |i| Vec3::splat(lut[i])).x
}

/// 0 for non-finite input, else `x` clamped to 0..=1.
fn clean(x: f32) -> f32 {
    if x.is_finite() {
        x.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Bilinear read of a `size`×`size` table at texel centers (columns N·V, rows roughness).
fn bilinear(size: usize, nov: f32, roughness: f32, at: impl Fn(usize) -> Vec3) -> Vec3 {
    let coord = |x: f32| (clean(x) * size as f32 - 0.5).clamp(0.0, (size - 1) as f32);
    let (x, y) = (coord(nov), coord(roughness));
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(size - 1), (y0 + 1).min(size - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let row = |y: usize| at(y * size + x0).lerp(at(y * size + x1), fx);
    row(y0).lerp(row(y1), fy)
}
