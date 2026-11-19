//! Buffer, texture and heap descriptors, validated before reaching a backend.

use std::num::{NonZeroU32, NonZeroU64};

use crate::caps::SampleCount;
use crate::error::RhiError;
use crate::format::Format;

/// Largest buffer the RHI accepts (backends may allow less; see `DeviceCaps::max_buffer_bytes`).
const MAX_BUFFER_BYTES: u64 = 1 << 36;
/// Largest texture dimension.
const MAX_TEXTURE_DIM: u32 = 16_384;

bitflags::bitflags! {
    /// How a buffer is used.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct BufferUsage: u32 {
        /// Read or written by shaders (through the bindless table).
        const STORAGE = 1;
        /// Index buffer.
        const INDEX = 1 << 1;
        /// Indirect draw/dispatch arguments.
        const INDIRECT = 1 << 2;
        /// Copy source.
        const COPY_SRC = 1 << 3;
        /// Copy destination.
        const COPY_DST = 1 << 4;
        /// Gets a bindless index.
        const BINDLESS = 1 << 5;
    }
}

bitflags::bitflags! {
    /// How a texture is used.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct TextureUsage: u32 {
        /// Sampled by shaders.
        const SAMPLED = 1;
        /// Written by shaders.
        const STORAGE = 1 << 1;
        /// Color or depth render target.
        const RENDER_TARGET = 1 << 2;
        /// Copy source.
        const COPY_SRC = 1 << 3;
        /// Copy destination.
        const COPY_DST = 1 << 4;
        /// Gets a bindless index.
        const BINDLESS = 1 << 5;
    }
}

/// Where buffer memory lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryLocation {
    /// GPU only (fastest for the GPU; not CPU-visible).
    GpuOnly,
    /// CPU writes, GPU reads (staging, per-frame data).
    Upload,
    /// GPU writes, CPU reads (readback).
    Readback,
}

/// A buffer to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferDesc {
    /// Size in bytes.
    pub size: NonZeroU64,
    /// Usage flags (at least one).
    pub usage: BufferUsage,
    /// Memory location.
    pub memory: MemoryLocation,
    /// Debug label.
    pub label: String,
}

impl BufferDesc {
    /// Checks the descriptor.
    pub fn validate(&self) -> Result<(), RhiError> {
        if self.usage.is_empty() {
            return Err(RhiError::invalid(
                "buffer desc",
                format!("{:?} has no usage", self.label),
            ));
        }
        if self.size.get() > MAX_BUFFER_BYTES {
            return Err(RhiError::invalid(
                "buffer desc",
                format!(
                    "{:?} is {} bytes; the limit is {MAX_BUFFER_BYTES}",
                    self.label, self.size
                ),
            ));
        }
        Ok(())
    }
}

/// A 2D texture to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureDesc {
    /// Width in pixels.
    pub width: NonZeroU32,
    /// Height in pixels.
    pub height: NonZeroU32,
    /// Mip levels (1 = no mips).
    pub mip_levels: NonZeroU32,
    /// MSAA samples (multisampled textures have one mip).
    pub samples: SampleCount,
    /// Pixel format.
    pub format: Format,
    /// Usage flags (at least one).
    pub usage: TextureUsage,
    /// Debug label.
    pub label: String,
}

impl TextureDesc {
    /// Checks the descriptor.
    pub fn validate(&self) -> Result<(), RhiError> {
        let bad = |reason: String| {
            Err(RhiError::invalid(
                "texture desc",
                format!("{:?}: {reason}", self.label),
            ))
        };
        let (w, h) = (self.width.get(), self.height.get());
        if w > MAX_TEXTURE_DIM || h > MAX_TEXTURE_DIM {
            return bad(format!("{w}x{h} exceeds {MAX_TEXTURE_DIM}"));
        }
        let max_mips = 32 - w.max(h).leading_zeros();
        if self.mip_levels.get() > max_mips {
            return bad(format!(
                "{} mip levels for {w}x{h}; at most {max_mips}",
                self.mip_levels
            ));
        }
        if self.samples != SampleCount::X1 && self.mip_levels.get() != 1 {
            return bad("multisampled textures have exactly one mip level".into());
        }
        if self.usage.is_empty() {
            return bad("no usage".into());
        }
        if self.format.is_depth() && self.usage.contains(TextureUsage::STORAGE) {
            return bad("depth formats cannot be storage textures".into());
        }
        if self.samples != SampleCount::X1 && self.usage.contains(TextureUsage::STORAGE) {
            return bad("multisampled textures cannot be storage textures".into());
        }
        Ok(())
    }
}

/// Memory for placed (aliased) resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeapDesc {
    /// Size in bytes.
    pub size: NonZeroU64,
    /// Debug label.
    pub label: String,
}
