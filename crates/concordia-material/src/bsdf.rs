//! The CPU reference BSDF of one slab (ADR-0068): GGX specular with an optional haze lobe and thin
//! film, diffuse (wrapped for subsurface) under the interface, and a Charlie sheen over both.
//!
//! References: glTF 2.0 specification, Appendix B; Google Filament material documentation
//! (Apache-2.0); Burley, "Physically Based Shading at Disney" (SIGGRAPH 2012); Estevez & Kulla,
//! "Production Friendly Microfacet Sheen BRDF" (SIGGRAPH 2017); Green, "Real-Time Approximations
//! to Subsurface Scattering" (GPU Gems, ch. 16; wrap lighting); Epic Games' public Substrate
//! documentation and the SIGGRAPH 2023 Substrate course (slab parameterization).

use std::f32::consts::{PI, TAU};

use concordia_assets::{Fuzz, Slab};
use concordia_core::math::Vec3;

use crate::dfg::{direct_albedo, sheen_albedo};
use crate::film::thin_film_reflectance;

/// Mean free path (metres) at which subsurface wrap reaches one half.
pub const SSS_SCALE: f32 = 0.01;

/// BSDF × N·L of `slab` for view `v` and light `l` around normal `n` (any lengths; zero on bad input).
pub fn eval_slab(slab: &Slab, n: Vec3, v: Vec3, l: Vec3) -> Vec3 {
    let (n, v, l) = (
        n.normalize_or_zero(),
        v.normalize_or_zero(),
        l.normalize_or_zero(),
    );
    if n == Vec3::ZERO || v == Vec3::ZERO || l == Vec3::ZERO {
        return Vec3::ZERO;
    }
    let h = (v + l).normalize_or_zero();
    let g = Geometry {
        nov: n.dot(v).abs() + 1e-5,
        nol_raw: n.dot(l),
        noh: n.dot(h).clamp(0.0, 1.0),
        voh: v.dot(h).clamp(0.0, 1.0),
    };
    let base = interface(slab, &g) + under_interface(slab, &g);
    match &slab.fuzz {
        Some(fuzz) => with_fuzz(fuzz, &g, base),
        None => base,
    }
}

/// Fraction of light passing through `slab` along a direction at cosine `cos` to the normal.
pub fn slab_transmittance(slab: &Slab, cos: f32) -> Vec3 {
    let mu = if cos.is_finite() {
        cos.abs().clamp(1e-4, 1.0)
    } else {
        1e-4
    };
    let reflected = specular_energy(slab, mu);
    let medium = Vec3::from_array(slab.transmittance)
        .clamp(Vec3::ZERO, Vec3::ONE)
        .powf(1.0 / mu);
    let sheen = slab.fuzz.map_or(1.0, |f| {
        1.0 - max3(f.color) * sheen_albedo(mu, f.roughness).min(1.0)
    });
    ((Vec3::ONE - reflected) * medium * sheen).clamp(Vec3::ZERO, Vec3::ONE)
}

/// Normal-incidence reflectance: dielectric 0.08 × specular, or the base color for metals.
pub(crate) fn f0(slab: &Slab) -> Vec3 {
    let base = rgb(slab);
    Vec3::splat(0.08 * slab.specular).lerp(base, slab.metallic)
}

/// The Charlie sheen lobe × N·L with the Estevez–Kulla visibility (perceptual roughness).
pub(crate) fn sheen_lobe(nov: f32, nol: f32, noh: f32, roughness: f32) -> f32 {
    if nol <= 0.0 || nov <= 0.0 {
        return 0.0;
    }
    let (nov, nol) = (nov.min(1.0), nol.min(1.0));
    let a = alpha(roughness);
    let d = (2.0 + 1.0 / a) * (1.0 - noh * noh).max(0.0).powf(0.5 / a) / TAU;
    d / (4.0 * nov * (1.0 + lambda(nov, a) + lambda(nol, a)))
}

struct Geometry {
    nov: f32,
    nol_raw: f32,
    noh: f32,
    voh: f32,
}

impl Geometry {
    fn nol(&self) -> f32 {
        self.nol_raw.max(0.0)
    }
}

/// Specular lobes × N·L (zero below the horizon).
fn interface(slab: &Slab, g: &Geometry) -> Vec3 {
    if g.nol() <= 0.0 {
        return Vec3::ZERO;
    }
    let ggx = |roughness: f32| {
        let a = alpha(roughness);
        d_ggx(g.noh, a) * v_smith(g.nov, g.nol(), a)
    };
    let lobe = match slab.haze {
        Some(h) => (1.0 - h.weight) * ggx(slab.roughness) + h.weight * ggx(h.roughness),
        None => ggx(slab.roughness),
    };
    fresnel(slab, g.voh) * lobe * g.nol()
}

/// Diffuse under the interface (Lambert × N·L, or wrapped per channel for subsurface), scaled by
/// what the interface doesn't reflect toward the viewer (albedo scaling; energy-conserving).
fn under_interface(slab: &Slab, g: &Geometry) -> Vec3 {
    let albedo =
        (1.0 - slab.metallic) * rgb(slab) * (Vec3::ONE - Vec3::from_array(slab.transmittance));
    let term = match &slab.subsurface {
        Some(s) => Vec3::from_array(s.mean_free_path.map(|mfp| wrap(g.nol_raw, mfp))),
        None => Vec3::splat(g.nol()),
    };
    (Vec3::ONE - specular_energy(slab, g.nov.min(1.0))) * albedo / PI * term
}

/// Directional albedo of the slab's specular lobes at view cosine `mu` (direct-lobe table): the
/// light the interface reflects, so diffuse and lower layers get only the rest.
fn specular_energy(slab: &Slab, mu: f32) -> Vec3 {
    let f0 = match &slab.thin_film {
        Some(film) => thin_film_reflectance(f0(slab), mu, film),
        None => f0(slab),
    };
    let energy = |roughness: f32| {
        let [a, b] = direct_albedo(mu, roughness);
        f0 * a + Vec3::splat(b)
    };
    match slab.haze {
        Some(h) => energy(slab.roughness).lerp(energy(h.roughness), h.weight),
        None => energy(slab.roughness),
    }
}

/// Energy-conserving wrapped diffuse: max(cos + w, 0) / (1 + w)², w = mfp / (mfp + scale).
fn wrap(cos: f32, mean_free_path: f32) -> f32 {
    let mfp = mean_free_path.max(0.0);
    let w = mfp / (mfp + SSS_SCALE);
    (cos + w).max(0.0) / ((1.0 + w) * (1.0 + w))
}

/// Sheen over the base, which is scaled by the sheen's tabulated albedo (energy-conserving).
fn with_fuzz(fuzz: &Fuzz, g: &Geometry, base: Vec3) -> Vec3 {
    let e = sheen_albedo(g.nov, fuzz.roughness);
    let sheen = Vec3::from_array(fuzz.color) * sheen_lobe(g.nov, g.nol(), g.noh, fuzz.roughness)
        / e.max(1.0);
    sheen + (1.0 - max3(fuzz.color) * e.min(1.0)) * base
}

fn fresnel(slab: &Slab, voh: f32) -> Vec3 {
    let f0 = f0(slab);
    match &slab.thin_film {
        Some(film) => thin_film_reflectance(f0, voh, film),
        None => f0 + (Vec3::ONE - f0) * (1.0 - voh).powi(5),
    }
}

fn d_ggx(noh: f32, a: f32) -> f32 {
    let a2 = a * a;
    let f = noh * noh * (a2 - 1.0) + 1.0;
    a2 / (PI * f * f)
}

/// Height-correlated Smith visibility.
pub(crate) fn v_smith(nov: f32, nol: f32, a: f32) -> f32 {
    let a2 = a * a;
    let gv = nol * (nov * nov * (1.0 - a2) + a2).sqrt();
    let gl = nov * (nol * nol * (1.0 - a2) + a2).sqrt();
    0.5 / (gv + gl).max(1e-5)
}

/// Estevez–Kulla Λ for the sheen visibility.
fn lambda(cos: f32, a: f32) -> f32 {
    const P0: [f32; 5] = [25.3245, 3.32435, 0.16801, -1.27393, -4.85967];
    const P1: [f32; 5] = [21.5473, 3.82987, 0.19823, -1.97760, -4.32054];
    let t = (1.0 - a) * (1.0 - a);
    let p: [f32; 5] = std::array::from_fn(|i| (1.0 - t) * P1[i] + t * P0[i]);
    let l = |x: f32| p[0] / (1.0 + p[1] * x.powf(p[2])) + p[3] * x + p[4];
    if cos < 0.5 {
        l(cos).exp()
    } else {
        (2.0 * l(0.5) - l(1.0 - cos)).exp()
    }
}

/// GGX alpha from perceptual roughness (clamped like M1's shader).
pub(crate) fn alpha(roughness: f32) -> f32 {
    let r = roughness.clamp(0.045, 1.0);
    r * r
}

fn rgb(slab: &Slab) -> Vec3 {
    let [r, g, b, _] = slab.base_color;
    Vec3::new(r, g, b)
}

fn max3(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2])
}
