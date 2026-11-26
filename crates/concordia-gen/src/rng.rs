//! A seeded SplitMix64 stream for layouts and shape parameters (integer arithmetic only, so it
//! is the same on every platform).

/// Seeded random numbers.
#[derive(Debug, Clone)]
pub(crate) struct Rng(u64);

impl Rng {
    /// The stream for `seed`.
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in [0, 1) with 24 random bits.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A number in [lo, hi).
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stream repeats for a seed, differs between seeds and stays in its range.
    #[test]
    fn stream_is_seeded_and_in_range() {
        let take = |seed| {
            let mut r = Rng::new(seed);
            (0..64).map(|_| r.range(-2.0, 3.0)).collect::<Vec<_>>()
        };
        assert_eq!(take(1), take(1));
        assert_ne!(take(1), take(2));
        assert!(take(1).iter().all(|v| (-2.0..3.0).contains(v)));
    }
}
