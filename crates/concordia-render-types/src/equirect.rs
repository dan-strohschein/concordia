//! The sky's equirectangular mapping: u = 0.5 at −Z, increasing toward +X; v = 0 straight up.

use std::f32::consts::{PI, TAU};

use concordia_core::math::Vec3;

/// Direction → (u, v).
pub fn dir_to_uv(d: Vec3) -> [f32; 2] {
    let d = d.normalize_or(Vec3::NEG_Z);
    [
        0.5 + d.x.atan2(-d.z) / TAU,
        d.y.clamp(-1.0, 1.0).acos() / PI,
    ]
}

/// (u, v) → unit direction (inverse of [`dir_to_uv`]).
pub fn uv_to_dir([u, v]: [f32; 2]) -> Vec3 {
    let (phi, theta) = ((u - 0.5) * TAU, v * PI);
    Vec3::new(
        theta.sin() * phi.sin(),
        theta.cos(),
        -theta.sin() * phi.cos(),
    )
}
