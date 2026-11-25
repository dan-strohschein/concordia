//! Dev-only test helpers: GPU test guards and golden-image comparison with NVIDIA FLIP.

pub mod golden;
pub mod reference;

/// Environment variable that makes GPU tests fail instead of skipping when no device exists.
pub const REQUIRE_GPU_ENV: &str = "CONCORDIA_REQUIRE_GPU";

/// True if this run must have a GPU (`CONCORDIA_REQUIRE_GPU=1`).
pub fn gpu_required() -> bool {
    gpu_required_from(std::env::var(REQUIRE_GPU_ENV).ok().as_deref())
}

/// [`gpu_required`] for a given variable value; only exactly `"1"` requires a GPU.
pub fn gpu_required_from(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Environment variable that makes Vulkan tests fail instead of skipping without a Vulkan device
/// and validation layers (Linux CI on lavapipe; `CONCORDIA_REQUIRE_GPU` keeps meaning Metal).
pub const REQUIRE_VULKAN_ENV: &str = "CONCORDIA_REQUIRE_VULKAN";

/// True if this run must have a Vulkan device (`CONCORDIA_REQUIRE_VULKAN=1`).
pub fn vulkan_required() -> bool {
    vulkan_required_from(std::env::var(REQUIRE_VULKAN_ENV).ok().as_deref())
}

/// [`vulkan_required`] for a given variable value; only exactly `"1"` requires Vulkan.
pub fn vulkan_required_from(value: Option<&str>) -> bool {
    gpu_required_from(value)
}
