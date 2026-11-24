//! Seeded value noise (integer hashing, bilinear with smoothstep), for heightfields.

/// A 32-bit hash of a lattice point and the seed (xorshift-multiply mixing).
fn hash(x: i32, y: i32, seed: u64) -> u32 {
    let mut h = (seed as u32) ^ ((seed >> 32) as u32).rotate_left(16);
    for v in [x as u32, y as u32] {
        h ^= v.wrapping_mul(0x9E37_79B1);
        h ^= h >> 15;
        h = h.wrapping_mul(0x85EB_CA77);
        h ^= h >> 13;
        h = h.wrapping_mul(0xC2B2_AE3D);
        h ^= h >> 16;
    }
    h
}

/// The lattice value at (x, y) in [-1, 1].
fn lattice(x: i32, y: i32, seed: u64) -> f32 {
    hash(x, y, seed) as f32 / u32::MAX as f32 * 2.0 - 1.0
}

/// Smooth value noise in [-1, 1] at (x, y).
pub(crate) fn value(x: f32, y: f32, seed: u64) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let (ix, iy) = (x0 as i32, y0 as i32);
    let top = lattice(ix, iy, seed) * (1.0 - sx) + lattice(ix + 1, iy, seed) * sx;
    let bottom = lattice(ix, iy + 1, seed) * (1.0 - sx) + lattice(ix + 1, iy + 1, seed) * sx;
    top * (1.0 - sy) + bottom * sy
}
