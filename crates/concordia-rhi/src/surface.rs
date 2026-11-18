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

/// When a presented image reached the display (wall-clock time), or `None` if it never did
/// (e.g. the layer is not on screen).
pub type PresentedCallback = Box<dyn FnOnce(Option<std::time::SystemTime>) + Send>;

/// What [`crate::Device::present`] submits.
pub struct Present<'a> {
    /// Commands drawing into the acquired image.
    pub commands: &'a crate::CommandList,
    /// Timelines to signal when the GPU finishes these commands.
    pub signal: &'a [(crate::Timeline, u64)],
    /// Called once the image is on screen.
    pub on_presented: Option<PresentedCallback>,
}

impl<'a> Present<'a> {
    /// Just the commands: no signals, no callback.
    pub fn new(commands: &'a crate::CommandList) -> Self {
        Self {
            commands,
            signal: &[],
            on_presented: None,
        }
    }
}

impl std::fmt::Debug for Present<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Present")
            .field("commands", &self.commands.label)
            .field("signal", &self.signal)
            .field("on_presented", &self.on_presented.is_some())
            .finish()
    }
}
