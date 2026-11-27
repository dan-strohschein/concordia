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

/// `SimRng` is a `rand_core` RNG too (for APIs such as virtualized WASI randomness).
impl rand_core::TryRng for SimRng {
    type Error = std::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(self.0.next_u32())
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(self.0.next_u64())
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        self.0.fill_bytes(dst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_rng_is_a_rand_core_rng_with_the_same_stream() {
        fn draw(rng: &mut impl rand_core::Rng) -> (u64, [u8; 5]) {
            let mut bytes = [0; 5];
            let first = rng.next_u64();
            rng.fill_bytes(&mut bytes);
            (first, bytes)
        }
        let (first, bytes) = draw(&mut SimRng::new(9));
        let mut plain = SimRng::new(9);
        assert_eq!(first, plain.next_u64());
        let mut inner = ChaCha8Rng::seed_from_u64(9);
        inner.next_u64();
        let mut expected = [0; 5];
        inner.fill_bytes(&mut expected);
        assert_eq!(bytes, expected);
    }

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

/// Per-processor random seeds (moved from `concordia-script`, M4 4.12: AI processors use them too),
/// so processors never contend for `SimRng`.
/// The seed of processor `name` at `tick` in a session seeded `session`: the first 8 bytes of
/// blake3 over the three, length-delimited.
pub fn processor_seed(session: u64, tick: u64, name: &str) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&session.to_le_bytes());
    hasher.update(&tick.to_le_bytes());
    hasher.update(&(name.len() as u64).to_le_bytes());
    hasher.update(name.as_bytes());
    let mut first = [0u8; 8];
    first.copy_from_slice(&hasher.finalize().as_bytes()[..8]);
    u64::from_le_bytes(first)
}

#[cfg(test)]
mod seed_tests {
    use super::*;

    #[test]
    fn seeds_differ_by_session_tick_and_processor() {
        let base = processor_seed(1, 10, "a.p");
        assert_ne!(base, processor_seed(2, 10, "a.p"));
        assert_ne!(base, processor_seed(1, 11, "a.p"));
        assert_ne!(base, processor_seed(1, 10, "a.q"));
        assert_eq!(base, processor_seed(1, 10, "a.p"));
    }

    #[test]
    fn seed_encoding_is_pinned() {
        let mut h = blake3::Hasher::new();
        h.update(&1u64.to_le_bytes());
        h.update(&10u64.to_le_bytes());
        h.update(&3u64.to_le_bytes());
        h.update(b"a.p");
        let expected = u64::from_le_bytes(h.finalize().as_bytes()[..8].try_into().unwrap());
        assert_eq!(processor_seed(1, 10, "a.p"), expected);
    }
}
