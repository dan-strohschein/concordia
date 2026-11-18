//! Concordia's Metal backend (ADR-0040). M1 phase 1.2a ships only the device probe.
//!
//! Compiles everywhere; off macOS every entry point reports [`RhiError::NoDevice`].

#[cfg(target_os = "macos")]
mod probe;

use concordia_rhi::{DeviceCaps, RhiError};

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
