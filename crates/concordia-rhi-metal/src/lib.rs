//! Concordia's Metal backend (ADR-0040): `MetalDevice` implements `concordia_rhi::Device` over
//! `objc2-metal` (Metal 3 API, tracked hazards, bindless argument-buffer tables).
//!
//! Compiles everywhere; off macOS every entry point reports [`RhiError::NoDevice`].

#[cfg(target_os = "macos")]
mod accel;
mod bindless;
#[cfg(target_os = "macos")]
mod convert;
#[cfg(target_os = "macos")]
mod device;
#[cfg(target_os = "macos")]
mod encode;
#[cfg(target_os = "macos")]
mod metalfx;
#[cfg(target_os = "macos")]
mod probe;
pub mod record;
#[cfg(target_os = "macos")]
mod surface;
#[cfg(target_os = "macos")]
mod timing;

use concordia_rhi::{RhiCaps, RhiError};

/// Bindless table capacity (textures and buffers each).
pub const BINDLESS_CAPACITY: u32 = 16_384;

/// Buffer slots shaders use: bindless buffers, sampled textures, storage textures, push constants.
pub mod slots {
    /// `constant BufferTable&` (device pointers).
    pub const BUFFERS: usize = 0;
    /// `constant TextureTable&` (sampled textures).
    pub const TEXTURES: usize = 1;
    /// `constant StorageTable&` (read-write textures; same table as `TEXTURES`).
    pub const STORAGE_TEXTURES: usize = 2;
    /// Push constants (`setBytes`).
    pub const PUSH: usize = 3;
    /// The acceleration-structure table (`raytracing.slang`'s `g_accels`, M4 4.8c).
    pub const ACCELS: usize = 4;
}

#[cfg(target_os = "macos")]
pub use device::MetalDevice;
#[cfg(target_os = "macos")]
pub use surface::{ViewLayer, layer_for_view};

/// Capabilities of the system default Metal device.
pub fn probe() -> Result<RhiCaps, RhiError> {
    #[cfg(target_os = "macos")]
    {
        probe::system_default()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(RhiError::NoDevice {
            reason: "the Metal backend is only available on macOS".into(),
        })
    }
}

/// What the system default Metal device reports (`concordia gpu-probe --record`).
pub fn probe_record() -> Result<record::ProbeRecord, RhiError> {
    #[cfg(target_os = "macos")]
    {
        probe::system_default_record()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(RhiError::NoDevice {
            reason: "the Metal backend is only available on macOS".into(),
        })
    }
}

/// The system default Metal device as an RHI device, without a window.
pub fn new_headless() -> Result<Box<dyn concordia_rhi::Device>, RhiError> {
    new_headless_masked(&concordia_rhi::CapsMask::default())
}

/// [`new_headless`] with `mask`'s features hidden: its caps report them missing and the device
/// refuses them, so one machine can test every fallback path.
pub fn new_headless_masked(
    mask: &concordia_rhi::CapsMask,
) -> Result<Box<dyn concordia_rhi::Device>, RhiError> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(MetalDevice::system_default_masked(mask)?))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = mask;
        Err(RhiError::NoDevice {
            reason: "the Metal backend is only available on macOS".into(),
        })
    }
}
