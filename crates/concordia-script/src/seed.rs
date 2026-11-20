//! Per-processor random seeds, so script processors never contend for `SimRng`.

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
mod tests {
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
