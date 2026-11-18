//! Proves crate integration tests are discovered and link against the crate.

#[test]
fn seed_crate_links_from_integration_test() {
    assert!(!concordia_core::version().is_empty());
}
