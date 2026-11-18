//! Concordia core: math, ids, time, logging and jobs (M0). Currently a seed.

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
