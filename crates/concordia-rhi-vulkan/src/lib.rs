//! Concordia's Vulkan backend (ADR-0088): `VulkanDevice` implements `concordia_rhi::Device` over
//! `ash` on Vulkan 1.3 (dynamic rendering, synchronization2, timeline semaphores, descriptor
//! indexing, buffer device addresses), with backend-tracked hazards like Metal's.
//!
//! The loader is opened at run time, so this builds everywhere; without a loader or a suitable
//! device every entry point reports [`RhiError::NoDevice`].
#![deny(clippy::undocumented_unsafe_blocks)]

mod bindless;
mod convert;
mod device;
mod encode;
mod instance;
mod pipeline;
pub mod record;
mod resources;
mod spirv;
mod swapchain;
mod timing;
mod track;
mod validation;
mod wsi;

use concordia_rhi::{CapsMask, Device, RhiCaps, RhiError};

pub use device::VulkanDevice;
pub use record::BINDLESS_CAPACITY;
pub use validation::ValidationLog;

/// Whether to run with the Khronos validation layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Validation {
    /// No layer.
    #[default]
    Off,
    /// The layer when it is installed.
    IfAvailable,
    /// The layer, or `NoDevice` without it (CI under `CONCORDIA_REQUIRE_VULKAN=1`).
    Required,
}

/// How to open a device.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Validation layer.
    pub validation: Validation,
    /// Features to hide (testing fallbacks).
    pub mask: CapsMask,
    /// Where validation errors go (a fresh log when `None`).
    pub log: Option<ValidationLog>,
    /// Any validation error makes the device's next submit, present, wait or read fail.
    pub strict: bool,
}

/// Environment variable choosing validation for devices the engine opens: `off` (default),
/// `auto` (the layer when installed), `on` (required) or `strict` (required, and errors fail).
pub const VALIDATION_ENV: &str = "CONCORDIA_VULKAN_VALIDATION";

impl Options {
    /// Options from this process's `CONCORDIA_VULKAN_VALIDATION`.
    pub fn from_env() -> Result<Self, RhiError> {
        Self::from_env_value(std::env::var(VALIDATION_ENV).ok().as_deref())
    }

    /// Options for a `CONCORDIA_VULKAN_VALIDATION` value (empty is unset).
    pub fn from_env_value(value: Option<&str>) -> Result<Self, RhiError> {
        let (validation, strict) = match value.unwrap_or("") {
            "" | "off" => (Validation::Off, false),
            "auto" => (Validation::IfAvailable, false),
            "on" => (Validation::Required, false),
            "strict" => (Validation::Required, true),
            other => {
                return Err(RhiError::invalid(
                    "options",
                    format!("{VALIDATION_ENV}={other:?}; expected off, auto, on or strict"),
                ));
            }
        };
        Ok(Self {
            validation,
            strict,
            ..Self::default()
        })
    }
}

/// Capabilities of the device [`new_headless`] would open.
pub fn probe() -> Result<RhiCaps, RhiError> {
    probe_record().map(|r| record::caps_from_record(&r))
}

/// What the probe reads from the device [`new_headless`] would open.
pub fn probe_record() -> Result<record::ProbeRecord, RhiError> {
    device::probe_record(Validation::Off)
}

/// The best Vulkan device as an RHI device, validated as `CONCORDIA_VULKAN_VALIDATION` says.
pub fn new_headless() -> Result<Box<dyn Device>, RhiError> {
    new_headless_masked(&CapsMask::default())
}

/// [`new_headless`] with `mask`'s features hidden: its caps report them missing and the device
/// refuses them.
pub fn new_headless_masked(mask: &CapsMask) -> Result<Box<dyn Device>, RhiError> {
    new_headless_with(&Options {
        mask: *mask,
        ..Options::from_env()?
    })
}

/// The best Vulkan device opened with `options`.
pub fn new_headless_with(options: &Options) -> Result<Box<dyn Device>, RhiError> {
    Ok(Box::new(VulkanDevice::new(options)?))
}
