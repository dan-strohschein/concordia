//! Concordia's render hardware interface: backend-neutral types and traits (ADR-0040).
//!
//! Backends (`concordia-rhi-metal`, later `concordia-rhi-vulkan`) implement it;
//! nothing here depends on a graphics API.

mod caps;
mod error;

pub use caps::{Backend, DeviceCaps, Requirement, SampleCount};
pub use error::RhiError;
