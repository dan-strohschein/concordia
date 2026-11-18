//! Seeded, reproducible randomness for simulation code.

use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

/// Deterministic simulation RNG (ChaCha8). Sim code never uses OS randomness.
///
/// ```
/// use concordia_core::rng::SimRng;
/// let (mut a, mut b) = (SimRng::new(7), SimRng::new(7));
/// assert_eq!(a.next_u64(), b.next_u64());
/// ```
#[derive(Debug, Clone)]
pub struct SimRng(ChaCha8Rng);

impl SimRng {
    /// Creates stream 0 for `seed`.
    pub fn new(seed: u64) -> Self {
        Self(ChaCha8Rng::seed_from_u64(seed))
    }

    /// An independent stream with the same seed; depends only on the seed and `stream`.
    pub fn fork(&self, stream: u64) -> Self {
        let mut rng = ChaCha8Rng::from_seed(self.0.get_seed());
        rng.set_stream(stream);
        Self(rng)
    }

    /// Next uniformly random `u32`.
    pub fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    /// Next uniformly random `u64`.
    pub fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    /// Next `f32` uniformly in `[0, 1)`, using 24 random bits.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// The 32-byte ChaCha key (identifies the seed; for state hashing).
    pub fn seed(&self) -> [u8; 32] {
        self.0.get_seed()
    }

    /// Current stream id.
    pub fn stream(&self) -> u64 {
        self.0.get_stream()
    }

    /// Position in the stream, in 32-bit words (for state hashing).
    pub fn word_pos(&self) -> u128 {
        self.0.get_word_pos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(rng: &mut SimRng, n: usize) -> Vec<u64> {
        (0..n).map(|_| rng.next_u64()).collect()
    }

    #[test]
    fn sim_rng_same_seed_same_stream() {
        assert_eq!(take(&mut SimRng::new(5), 64), take(&mut SimRng::new(5), 64));
        assert_ne!(take(&mut SimRng::new(5), 8), take(&mut SimRng::new(6), 8));
    }

    #[test]
    fn sim_rng_streams_split_deterministically() {
        let mut parent = SimRng::new(5);
        let before = take(&mut parent.fork(1), 16);
        take(&mut parent, 100);
        assert_eq!(
            take(&mut parent.fork(1), 16),
            before,
            "fork ignores parent position"
        );
        assert_ne!(take(&mut parent.fork(2), 16), before);
        assert_eq!(parent.fork(3).stream(), 3);
    }

    #[test]
    fn seed_identifies_the_key_not_the_position() {
        let mut a = SimRng::new(3);
        let seed = a.seed();
        a.next_u64();
        assert_eq!(a.seed(), seed);
        assert_ne!(SimRng::new(4).seed(), seed);
        assert_eq!(a.fork(2).seed(), seed);
    }

    #[test]
    fn next_f32_is_in_unit_interval() {
        let mut rng = SimRng::new(1);
        assert!(
            (0..10_000)
                .map(|_| rng.next_f32())
                .all(|x| (0.0..1.0).contains(&x))
        );
    }
}
