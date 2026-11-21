//! Low-discrepancy sampling shared by the directional-albedo tables and image-based lighting.

use std::f32::consts::TAU;

use concordia_core::math::Vec3;

/// The i-th of `n` Hammersley points (`n` > 0).
pub fn hammersley(i: u32, n: u32) -> (f32, f32) {
    (
        i as f32 / n.max(1) as f32,
        i.reverse_bits() as f32 * 2.328_306_4e-10,
    )
}

/// A GGX-distributed half vector around `n` (alpha = roughness²).
pub fn importance_ggx((u, v): (f32, f32), n: Vec3, alpha: f32) -> Vec3 {
    let phi = TAU * u;
    let cos_t = ((1.0 - v) / (1.0 + (alpha * alpha - 1.0) * v))
        .max(0.0)
        .sqrt();
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    let up = if n.z.abs() < 0.999 { Vec3::Z } else { Vec3::X };
    let tx = up.cross(n).normalize();
    let ty = n.cross(tx);
    (tx * (sin_t * phi.cos()) + ty * (sin_t * phi.sin()) + n * cos_t).normalize()
}
