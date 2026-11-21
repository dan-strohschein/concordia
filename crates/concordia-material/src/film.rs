//! Thin-film interference: Airy reflectance of one dielectric film over the slab's interface,
//! sampled at one wavelength per color channel.
//!
//! References: Born & Wolf, "Principles of Optics" (thin-film Airy summation); Belcour & Barla,
//! "A Practical Extension to Microfacet Theory for the Modeling of Varying Iridescence" (SIGGRAPH 2017).

use concordia_assets::ThinFilm;
use concordia_core::math::Vec3;

/// Wavelengths sampled for red, green and blue, in nanometres.
pub const WAVELENGTHS_NM: [f32; 3] = [630.0, 532.0, 465.0];

/// Reflectance at incidence cosine `cos` of `film` over a base whose normal-incidence
/// reflectance is `f0` (its index of refraction is derived per channel).
pub fn thin_film_reflectance(f0: Vec3, cos: f32, film: &ThinFilm) -> Vec3 {
    let cos = if cos.is_finite() {
        cos.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let n1 = film.ior.max(1.0);
    let d = film.thickness_nm.max(0.0);
    Vec3::from_array(std::array::from_fn(|c| {
        let s = f0[c].clamp(0.0, 1.0).sqrt().min(0.99);
        airy(cos, n1, (1.0 + s) / (1.0 - s), d, WAVELENGTHS_NM[c])
    }))
}

/// Unpolarized Airy reflectance: air → film (`n1`, thickness `d` nm) → base (`n2`), at `lambda` nm.
fn airy(cos0: f32, n1: f32, n2: f32, d: f32, lambda: f32) -> f32 {
    let sin0 = (1.0 - cos0 * cos0).max(0.0).sqrt();
    let cos_in = |n: f32| (1.0 - (sin0 / n).powi(2)).max(0.0).sqrt();
    let (cos1, cos2) = (cos_in(n1), cos_in(n2));
    let r01s = (cos0 - n1 * cos1) / (cos0 + n1 * cos1);
    let r01p = (n1 * cos0 - cos1) / (n1 * cos0 + cos1);
    let r12s = (n1 * cos1 - n2 * cos2) / (n1 * cos1 + n2 * cos2);
    let r12p = (n2 * cos1 - n1 * cos2) / (n2 * cos1 + n1 * cos2);
    let phase = (4.0 * std::f32::consts::PI * n1 * d * cos1 / lambda).cos();
    let r = |a: f32, b: f32| {
        (a * a + b * b + 2.0 * a * b * phase) / (1.0 + a * a * b * b + 2.0 * a * b * phase)
    };
    (0.5 * (r(r01s, r12s) + r(r01p, r12p))).clamp(0.0, 1.0)
}
