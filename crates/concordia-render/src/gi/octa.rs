//! Octahedral mapping of directions to the square (Cigolle et al. 2014).

use concordia_core::math::{UVec2, Vec2, Vec3};

/// `d` on the [-1, 1]² square.
pub fn encode(d: Vec3) -> Vec2 {
    let l1 = d.x.abs() + d.y.abs() + d.z.abs();
    if !(l1.is_finite() && l1 > 0.0) {
        return Vec2::ZERO;
    }
    let n = d / l1;
    let p = Vec2::new(n.x, n.y);
    if n.z >= 0.0 {
        p
    } else {
        (Vec2::ONE - Vec2::new(p.y.abs(), p.x.abs())) * sign(p)
    }
}

/// The unit direction at `p` on the square.
pub fn decode(p: Vec2) -> Vec3 {
    let p = p.clamp(Vec2::splat(-1.0), Vec2::ONE);
    let z = 1.0 - p.x.abs() - p.y.abs();
    let xy = if z >= 0.0 {
        p
    } else {
        (Vec2::ONE - Vec2::new(p.y.abs(), p.x.abs())) * sign(p)
    };
    Vec3::new(xy.x, xy.y, z).normalize_or(Vec3::Z)
}

/// The direction through the centre of texel `t` of a `size`² map (no border).
pub fn texel_dir(t: UVec2, size: u32) -> Vec3 {
    let size = size.max(1) as f32;
    decode((t.as_vec2() + 0.5) / size * 2.0 - Vec2::ONE)
}

/// The texel of a `size`² map that direction `d` falls in.
pub fn texel_of(d: Vec3, size: u32) -> UVec2 {
    let last = size.max(1) - 1;
    let t = ((encode(d) + Vec2::ONE) * 0.5 * size.max(1) as f32).floor();
    UVec2::new(
        (t.x.max(0.0) as u32).min(last),
        (t.y.max(0.0) as u32).min(last),
    )
}

/// ±1 per component (0 counts as +1, so the fold is defined on the axes).
fn sign(p: Vec2) -> Vec2 {
    Vec2::new(
        if p.x >= 0.0 { 1.0 } else { -1.0 },
        if p.y >= 0.0 { 1.0 } else { -1.0 },
    )
}
