//! The device: creates resources and pipelines, submits command lists.

use std::time::Duration;

use crate::caps::DeviceCaps;
use crate::command::CommandList;
use crate::error::RhiError;
use crate::handle::{BindlessIndex, Buffer, Heap, Pipeline, Texture, Timeline};
use crate::pipeline::{ComputePipelineDesc, GraphicsPipelineDesc, MeshPipelineDesc};
use crate::resource::{BufferDesc, HeapDesc, TextureDesc};

/// Which hardware queue runs a submission (M1 runs everything on `Graphics`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueueKind {
    /// Graphics and everything else.
    Graphics,
    /// Async compute.
    Compute,
    /// Copies.
    Transfer,
}

/// One submission: commands plus timeline waits and signals.
#[derive(Debug, Clone, Copy)]
pub struct Submit<'a> {
    /// Queue.
    pub queue: QueueKind,
    /// Commands (validated before translation).
    pub commands: &'a CommandList,
    /// Wait for each timeline to reach its value first.
    pub wait: &'a [(Timeline, u64)],
    /// Signal each timeline to its value when done.
    pub signal: &'a [(Timeline, u64)],
}

/// A GPU device. Object-safe so the conformance suite runs against any backend.
pub trait Device: Send + Sync {
    /// Capabilities.
    fn caps(&self) -> &DeviceCaps;
    /// GPU memory this device currently has allocated, in bytes, if the backend can tell.
    fn allocated_bytes(&self) -> Option<u64> {
        None
    }
    /// Creates a buffer.
    fn create_buffer(&self, desc: &BufferDesc) -> Result<Buffer, RhiError>;
    /// Creates a texture.
    fn create_texture(&self, desc: &TextureDesc) -> Result<Texture, RhiError>;
    /// Creates memory for placed textures.
    fn create_heap(&self, desc: &HeapDesc) -> Result<Heap, RhiError>;
    /// Size and alignment a texture needs inside a heap.
    fn texture_heap_footprint(&self, desc: &TextureDesc) -> Result<(u64, u64), RhiError>;
    /// Creates a texture at `offset` in `heap` (it may alias other placed textures).
    fn create_placed_texture(
        &self,
        heap: Heap,
        offset: u64,
        desc: &TextureDesc,
    ) -> Result<Texture, RhiError>;
    /// Creates a compute pipeline.
    fn create_compute_pipeline(&self, desc: &ComputePipelineDesc) -> Result<Pipeline, RhiError>;
    /// Creates a graphics pipeline.
    fn create_graphics_pipeline(&self, desc: &GraphicsPipelineDesc) -> Result<Pipeline, RhiError>;
    /// Creates a mesh pipeline, or `Unsupported` unless `caps().mesh_shaders`.
    fn create_mesh_pipeline(&self, desc: &MeshPipelineDesc) -> Result<Pipeline, RhiError>;
    /// Writes CPU data into an `Upload` or `Readback` buffer.
    fn write_buffer(&self, buffer: Buffer, offset: u64, data: &[u8]) -> Result<(), RhiError>;
    /// Reads a CPU-visible buffer (wait for the GPU first).
    fn read_buffer(&self, buffer: Buffer, offset: u64, len: u64) -> Result<Vec<u8>, RhiError>;
    /// The bindless slot of a buffer created with `BufferUsage::BINDLESS`.
    fn buffer_index(&self, buffer: Buffer) -> Result<BindlessIndex, RhiError>;
    /// The bindless slot of a texture created with `TextureUsage::BINDLESS`.
    fn texture_index(&self, texture: Texture) -> Result<BindlessIndex, RhiError>;
    /// Creates a timeline at `initial`.
    fn create_timeline(&self, initial: u64) -> Result<Timeline, RhiError>;
    /// The timeline's completed value.
    fn timeline_value(&self, timeline: Timeline) -> Result<u64, RhiError>;
    /// Blocks until the timeline reaches `value` (false on timeout).
    fn wait_timeline(
        &self,
        timeline: Timeline,
        value: u64,
        timeout: Duration,
    ) -> Result<bool, RhiError>;
    /// Validates and submits work.
    fn submit(&self, submit: Submit<'_>) -> Result<(), RhiError>;
    /// Blocks until all submitted work finished.
    fn wait_idle(&self) -> Result<(), RhiError>;
    /// Releases a buffer once the GPU is done with it.
    fn destroy_buffer(&self, buffer: Buffer);
    /// Releases a texture once the GPU is done with it.
    fn destroy_texture(&self, texture: Texture);
    /// Releases a heap once the GPU is done with it (its placed textures must be destroyed first).
    fn destroy_heap(&self, heap: Heap);
    /// Wraps a native window layer for presenting in `format`.
    ///
    /// # Safety
    /// The native object behind `target` must stay alive until [`Device::destroy_surface`].
    unsafe fn create_surface(
        &self,
        target: crate::SurfaceTarget,
        format: crate::Format,
    ) -> Result<crate::Surface, RhiError>;
    /// The surface's next image as a render-target texture, sized `width`×`height` pixels; `None`
    /// when every image is still in use (skip the frame). One image at a time: present it before
    /// acquiring again.
    fn acquire(
        &self,
        surface: crate::Surface,
        width: u32,
        height: u32,
    ) -> Result<Option<Texture>, RhiError>;
    /// Submits the commands (which draw into the acquired image), signals the timelines when
    /// they finish, and presents the image; its texture handle is released.
    fn present(&self, surface: crate::Surface, present: crate::Present<'_>)
    -> Result<(), RhiError>;
    /// Releases a surface (and any image still acquired from it).
    fn destroy_surface(&self, surface: crate::Surface);
}
