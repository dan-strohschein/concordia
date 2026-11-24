//! Counter-based random numbers: each value is a hash of (seed, pixel, sample, dimension), so an
//! image never depends on which thread drew which pixel, or in what order (identical at 1 and 8
//! threads; the GPU backend uses the same hash).

/// The random numbers for one pixel's sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleStream {
    seed: u32,
    pixel: u32,
    sample: u32,
    dimension: u32,
}

impl SampleStream {
    /// The stream for `sample` of `pixel` under `seed`.
    pub fn new(seed: u32, pixel: u32, sample: u32) -> Self {
        Self {
            seed,
            pixel,
            sample,
            dimension: 0,
        }
    }

    /// The next value in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        let h = pcg(self.seed ^ pcg(self.pixel ^ pcg(self.sample ^ pcg(self.dimension))));
        self.dimension = self.dimension.wrapping_add(1);
        (h >> 8) as f32 * (1.0 / (1u32 << 24) as f32)
    }
}

/// The PCG-RXS-M-XS 32-bit hash (Jarzynski & Olano, "Hash Functions for GPU Rendering", JCGT 2020).
fn pcg(v: u32) -> u32 {
    let state = v.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277_803_737);
    (word >> 22) ^ word
}
