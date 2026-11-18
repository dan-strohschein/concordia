//! Synchronization2-style barriers: stages, access and layout before and after.

use crate::handle::{Buffer, Texture};

bitflags::bitflags! {
    /// Pipeline stages.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Stages: u32 {
        /// Indirect argument reads.
        const DRAW_INDIRECT = 1;
        /// Vertex (and object/mesh) shaders.
        const VERTEX = 1 << 1;
        /// Fragment shaders.
        const FRAGMENT = 1 << 2;
        /// Depth tests and writes.
        const DEPTH = 1 << 3;
        /// Color attachment output.
        const COLOR_OUTPUT = 1 << 4;
        /// Compute shaders.
        const COMPUTE = 1 << 5;
        /// Copies.
        const COPY = 1 << 6;
        /// Host access.
        const HOST = 1 << 7;
    }
}

bitflags::bitflags! {
    /// Kinds of memory access.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct AccessFlags: u32 {
        /// Shader reads (sampled or storage).
        const SHADER_READ = 1;
        /// Shader writes.
        const SHADER_WRITE = 1 << 1;
        /// Color attachment reads.
        const COLOR_READ = 1 << 2;
        /// Color attachment writes.
        const COLOR_WRITE = 1 << 3;
        /// Depth reads.
        const DEPTH_READ = 1 << 4;
        /// Depth writes.
        const DEPTH_WRITE = 1 << 5;
        /// Copy reads.
        const COPY_READ = 1 << 6;
        /// Copy writes.
        const COPY_WRITE = 1 << 7;
        /// Indirect argument reads.
        const INDIRECT_READ = 1 << 8;
        /// Index reads.
        const INDEX_READ = 1 << 9;
        /// Host reads.
        const HOST_READ = 1 << 10;
        /// Host writes.
        const HOST_WRITE = 1 << 11;
    }
}

/// Texture layout (a no-op on Metal; meaningful on Vulkan).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layout {
    /// Contents may be discarded.
    Undefined,
    /// Any access (slow path).
    General,
    /// Color attachment.
    ColorAttachment,
    /// Depth attachment.
    DepthAttachment,
    /// Read-only in shaders.
    ShaderRead,
    /// Copy source.
    CopySrc,
    /// Copy destination.
    CopyDst,
    /// Ready to present.
    Present,
}

impl Layout {
    /// Every layout, in order.
    pub const ALL: [Self; 8] = [
        Self::Undefined,
        Self::General,
        Self::ColorAttachment,
        Self::DepthAttachment,
        Self::ShaderRead,
        Self::CopySrc,
        Self::CopyDst,
        Self::Present,
    ];
}

/// How a resource is accessed on one side of a barrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Access {
    /// Stages that access it.
    pub stages: Stages,
    /// How.
    pub access: AccessFlags,
    /// Texture layout (ignored for buffers).
    pub layout: Layout,
}

impl Access {
    /// Combines two accesses of the same side: union of stages and flags; differing layouts fall back to `General`.
    pub fn merge(self, other: Self) -> Self {
        Self {
            stages: self.stages | other.stages,
            access: self.access | other.access,
            layout: if self.layout == other.layout {
                self.layout
            } else {
                Layout::General
            },
        }
    }
}

/// A buffer dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferBarrier {
    /// The buffer.
    pub buffer: Buffer,
    /// Access before.
    pub before: Access,
    /// Access after.
    pub after: Access,
}

/// A texture dependency (or an aliasing switch when `aliasing` is set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureBarrier {
    /// The texture.
    pub texture: Texture,
    /// Access before.
    pub before: Access,
    /// Access after.
    pub after: Access,
    /// The previous occupant of the same heap memory, if this begins an aliased resource's use.
    pub aliasing: Option<Texture>,
}
