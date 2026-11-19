//! Concordia core: ids, math, fixed-step time, logging, jobs, deterministic RNG and the DDC store.

pub mod ddc;
pub mod fs;
pub mod id;
pub mod jobs;
pub mod log;
pub mod math;
pub mod process;
pub mod rng;
pub mod suggest;
pub mod time;

/// The engine's name (ADR-0008).
pub const NAME: &str = "concordia";

/// The crate version.
///
/// ```
/// assert!(!concordia_core::version().is_empty());
/// ```
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_cargo_manifest() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn name_is_concordia() {
        assert_eq!(NAME, "concordia");
    }
}
