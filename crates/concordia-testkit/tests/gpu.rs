//! GPU test guards (M1 phase 1.2a).

use concordia_testkit::gpu_required_from;

#[test]
fn gpu_is_required_only_when_env_is_exactly_one() {
    assert!(gpu_required_from(Some("1")));
    for value in [None, Some("0"), Some(""), Some("true"), Some(" 1")] {
        assert!(!gpu_required_from(value), "{value:?}");
    }
}

#[test]
fn vulkan_is_required_only_when_its_env_is_exactly_one() {
    use concordia_testkit::{REQUIRE_VULKAN_ENV, vulkan_required_from};
    assert_eq!(REQUIRE_VULKAN_ENV, "CONCORDIA_REQUIRE_VULKAN");
    assert!(vulkan_required_from(Some("1")));
    for value in [None, Some("0"), Some(""), Some("true"), Some(" 1")] {
        assert!(!vulkan_required_from(value), "{value:?}");
    }
}

#[test]
fn each_backend_is_required_by_its_own_env() {
    use concordia_rhi::Backend;
    use concordia_testkit::device_required_from;
    assert!(device_required_from(Backend::Metal, Some("1"), None));
    assert!(!device_required_from(Backend::Metal, None, Some("1")));
    assert!(device_required_from(Backend::Vulkan, None, Some("1")));
    assert!(!device_required_from(Backend::Vulkan, Some("1"), None));
    assert!(!device_required_from(Backend::Vulkan, None, Some("true")));
}
