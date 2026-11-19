//! Concordia's Metal backend (ADR-0040): `MetalDevice` implements `concordia_rhi::Device` over
//! `objc2-metal` (Metal 3 API, tracked hazards, bindless argument-buffer tables).
//!
//! Compiles everywhere; off macOS every entry point reports [`RhiError::NoDevice`].

#[cfg(target_os = "macos")]
mod bindless;
#[cfg(target_os = "macos")]
mod convert;
#[cfg(target_os = "macos")]
mod device;
#[cfg(target_os = "macos")]
mod encode;
#[cfg(target_os = "macos")]
mod probe;
#[cfg(target_os = "macos")]
mod surface;

use concordia_rhi::{DeviceCaps, RhiError};

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
}

#[cfg(target_os = "macos")]
pub use device::MetalDevice;
#[cfg(target_os = "macos")]
pub use surface::{ViewLayer, layer_for_view};

/// Capabilities of the system default Metal device.
pub fn probe() -> Result<DeviceCaps, RhiError> {
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

/// The system default Metal device as an RHI device, without a window.
pub fn new_headless() -> Result<Box<dyn concordia_rhi::Device>, RhiError> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(MetalDevice::system_default()?))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(RhiError::NoDevice {
            reason: "the Metal backend is only available on macOS".into(),
        })
    }
}
