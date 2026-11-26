//! Seeded value noise (integer hashing, smoothstep interpolation) in 2D and 3D, and fractal sums
//! of it. Only `+ − × ÷` and `floor`, so the values are the same on every platform.

/// A 32-bit hash of lattice coordinates and the seed (xorshift-multiply mixing).
fn hash(coords: &[i32], seed: u64) -> u32 {
    let mut h = (seed as u32) ^ ((seed >> 32) as u32).rotate_left(16);
    for &v in coords {
        h ^= (v as u32).wrapping_mul(0x9E37_79B1);
        h ^= h >> 15;
        h = h.wrapping_mul(0x85EB_CA77);
        h ^= h >> 13;
        h = h.wrapping_mul(0xC2B2_AE3D);
        h ^= h >> 16;
    }
    h
}

/// The lattice value at `coords` in [-1, 1].
fn lattice(coords: &[i32], seed: u64) -> f32 {
    hash(coords, seed) as f32 / u32::MAX as f32 * 2.0 - 1.0
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

/// Smooth value noise in [-1, 1] at (x, y).
pub(crate) fn value(x: f32, y: f32, seed: u64) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (sx, sy) = (smooth(x - x0), smooth(y - y0));
    let (ix, iy) = (x0 as i32, y0 as i32);
    let top = lerp(lattice(&[ix, iy], seed), lattice(&[ix + 1, iy], seed), sx);
    let bottom = lerp(
        lattice(&[ix, iy + 1], seed),
        lattice(&[ix + 1, iy + 1], seed),
        sx,
    );
    lerp(top, bottom, sy)
}

/// Smooth value noise in [-1, 1] at `p`.
pub(crate) fn value3(p: [f32; 3], seed: u64) -> f32 {
    let base = p.map(f32::floor);
    let t: [f32; 3] = std::array::from_fn(|i| smooth(p[i] - base[i]));
    let [ix, iy, iz] = base.map(|c| c as i32);
    let at = |dx: i32, dy: i32, dz: i32| lattice(&[ix + dx, iy + dy, iz + dz], seed);
    let plane = |dz: i32| {
        let near = lerp(at(0, 0, dz), at(1, 0, dz), t[0]);
        let far = lerp(at(0, 1, dz), at(1, 1, dz), t[0]);
        lerp(near, far, t[1])
    };
    lerp(plane(0), plane(1), t[2])
}

/// `octaves` of 2D value noise, each twice the frequency and half the amplitude of the last,
/// normalized to [-1, 1].
pub(crate) fn fbm2(x: f32, y: f32, seed: u64, octaves: u32) -> f32 {
    fractal(octaves, |f, s| value(x * f, y * f, seed.wrapping_add(s)))
}

/// `octaves` of 3D value noise, as [`fbm2`].
pub(crate) fn fbm3(p: [f32; 3], seed: u64, octaves: u32) -> f32 {
    fractal(octaves, |f, s| {
        value3(p.map(|c| c * f), seed.wrapping_add(s))
    })
}

/// The normalized sum of `octave(frequency, index)` over `octaves` (at least one).
fn fractal(octaves: u32, octave: impl Fn(f32, u64) -> f32) -> f32 {
    let (mut sum, mut weight, mut amplitude, mut frequency) = (0.0, 0.0, 1.0, 1.0);
    for i in 0..octaves.max(1) {
        sum += amplitude * octave(frequency, u64::from(i));
        weight += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    sum / weight
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points() -> impl Iterator<Item = [f32; 3]> {
        (0..400).map(|i| {
            let f = i as f32;
            [f * 0.173 - 30.0, f * 0.091 - 11.0, f * 0.257 - 47.0]
        })
    }

    /// 3D noise lies in [-1, 1], equals its lattice values at lattice points, changes smoothly
    /// (a small step moves it a little), depends on the seed and repeats for the same one.
    #[test]
    fn value_noise_3d_is_smooth_and_seeded() {
        for p in points() {
            let v = value3(p, 9);
            assert!((-1.0..=1.0).contains(&v), "{p:?}: {v}");
            assert_eq!(v, value3(p, 9));
            let step = value3([p[0] + 1e-3, p[1], p[2]], 9);
            assert!((step - v).abs() < 1e-2, "{p:?}: {v} → {step}");
        }
        let at = [3.0, -2.0, 5.0];
        assert_eq!(value3(at, 9), lattice(&[3, -2, 5], 9));
        let differ = points().filter(|&p| value3(p, 9) != value3(p, 10)).count();
        assert!(differ > 390, "{differ} of 400 differ between seeds");
        let varied = points().filter(|&p| value3(p, 9).abs() > 0.2).count();
        assert!(varied > 100, "{varied} of 400 away from zero");
    }

    /// Fractal sums stay in [-1, 1], add detail (more octaves change the value) and are seeded.
    #[test]
    fn fractal_noise_is_bounded_and_seeded() {
        for p in points() {
            for octaves in 1..=5 {
                let v3 = fbm3(p, 4, octaves);
                let v2 = fbm2(p[0], p[2], 4, octaves);
                assert!((-1.0..=1.0).contains(&v3) && (-1.0..=1.0).contains(&v2));
            }
            assert_eq!(fbm3(p, 4, 1), value3(p, 4));
            assert_eq!(fbm2(p[0], p[2], 4, 1), value(p[0], p[2], 4));
        }
        let detail = points().filter(|&p| fbm3(p, 4, 4) != fbm3(p, 4, 1)).count();
        assert!(detail > 390, "{detail}");
        let seeded = points().filter(|&p| fbm2(p[0], p[1], 4, 3) != fbm2(p[0], p[1], 5, 3));
        assert!(seeded.count() > 390);
    }
}
