//! GPU test guards (M1 phase 1.2a).

use concordia_testkit::gpu_required_from;

#[test]
fn gpu_is_required_only_when_env_is_exactly_one() {
    assert!(gpu_required_from(Some("1")));
    for value in [None, Some("0"), Some(""), Some("true"), Some(" 1")] {
        assert!(!gpu_required_from(value), "{value:?}");
    }
}
