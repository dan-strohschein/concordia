//! Window surfaces: what a device presents into.

use std::ffi::c_void;
use std::ptr::NonNull;

/// The native object a surface wraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceTarget {
    /// A `CAMetalLayer` (macOS); the caller keeps it alive while the surface exists.
    MetalLayer(NonNull<c_void>),
}

// SAFETY: the pointer is only dereferenced by the device, under its lock, while the caller
// guarantees the layer is alive (see `Device::create_surface`).
unsafe impl Send for SurfaceTarget {}
