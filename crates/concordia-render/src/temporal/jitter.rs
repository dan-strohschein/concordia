//! The sub-pixel jitter sequence: Halton (2, 3) from index 1, in input pixels.

/// The radical inverse of `index` in `base` (Halton 1960), in [0, 1).
pub fn halton(index: u32, base: u32) -> f32 {
    debug_assert!(base >= 2, "base {base}");
    let (mut index, base) = (index, base.max(2));
    let (mut result, mut scale) = (0.0f64, 1.0f64);
    while index > 0 {
        scale /= f64::from(base);
        result += scale * f64::from(index % base);
        index /= base;
    }
    result as f32
}

/// Frame `frame`'s offset in [−0.5, 0.5)² input pixels (x right, y down), cycling every `phases`
/// frames; `phases` of 0 counts as 1.
pub fn jitter(frame: u64, phases: u32) -> [f32; 2] {
    let i = (frame % u64::from(phases.max(1))) as u32 + 1;
    [halton(i, 2) - 0.5, halton(i, 3) - 0.5]
}

/// `offset` pixels as a clip-space offset per unit `w` at `size` (NDC y is up).
pub fn ndc_offset(offset: [f32; 2], size: (u32, u32)) -> [f32; 2] {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    [2.0 * offset[0] / w, -2.0 * offset[1] / h]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_sequence_is_halton_2_3_and_zero_mean() {
        let expected2 = [0.5, 0.25, 0.75, 0.125, 0.625, 0.375, 0.875, 0.0625];
        let expected3 = [
            1.0 / 3.0,
            2.0 / 3.0,
            1.0 / 9.0,
            4.0 / 9.0,
            7.0 / 9.0,
            2.0 / 9.0,
        ];
        for (i, e) in expected2.iter().enumerate() {
            assert!((halton(i as u32 + 1, 2) - e).abs() < 1e-6, "base 2, {i}");
        }
        for (i, e) in expected3.iter().enumerate() {
            assert!((halton(i as u32 + 1, 3) - e).abs() < 1e-6, "base 3, {i}");
        }
        // Index 1 first (index 0 would be the pixel centre), then cycling.
        assert_eq!(jitter(0, 8), [halton(1, 2) - 0.5, halton(1, 3) - 0.5]);
        assert_eq!(jitter(8, 8), jitter(0, 8));
        assert_ne!(jitter(7, 8), jitter(0, 8));
        for phases in [8, 18, 32] {
            let cycle: Vec<_> = (0..phases as u64).map(|f| jitter(f, phases)).collect();
            for axis in 0..2 {
                let mean = cycle.iter().map(|j| j[axis]).sum::<f32>() / phases as f32;
                assert!(
                    mean.abs() <= 1.0 / phases as f32,
                    "{phases} phases, axis {axis}: {mean}"
                );
            }
            assert!(cycle.iter().flatten().all(|v| (-0.5..0.5).contains(v)));
        }
        assert_eq!(jitter(5, 0), jitter(0, 1));
    }

    #[test]
    fn ndc_offset_converts_pixels_with_y_up() {
        assert_eq!(ndc_offset([0.5, 0.25], (100, 50)), [0.01, -0.01]);
        assert_eq!(ndc_offset([0.0, 0.0], (7, 3)), [0.0, 0.0]);
    }
}
