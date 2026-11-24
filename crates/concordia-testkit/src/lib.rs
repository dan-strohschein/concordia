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
