//! Concordia's render hardware interface: backend-neutral types and the [`Device`] trait (ADR-0040).
//!
//! Resources are typed handles owned by the device. Work is recorded as data into a
//! [`CommandList`] and translated by the backend at [`Device::submit`], so the render graph and
//! tests can produce and inspect commands without a GPU. Shaders reach resources through one
//! bindless table per device and receive only [`BindlessIndex`]es (in push constants or buffers).

mod barrier;
mod caps;
mod command;
mod device;
mod error;
mod format;
mod handle;
mod pipeline;
mod resource;
mod slots;
mod surface;

pub mod check;
#[cfg(feature = "conformance")]
pub mod conformance;

pub use barrier::{Access, AccessFlags, BufferBarrier, Layout, Stages, TextureBarrier};
pub use caps::{
    Backend, CapsMask, Requirement, RhiCaps, SampleCount, TimestampGranularity, UnknownBackend,
};
pub use command::{
    ClearValue, ColorAttachment, Command, CommandList, DRAW_INDEXED_INDIRECT_BYTES,
    DRAW_INDIRECT_BYTES, DepthAttachment, LoadOp, MAX_MESH_TASK_GROUPS, MAX_MESH_TASK_GROUPS_TOTAL,
    MAX_PUSH_CONSTANTS, Rect, RenderPassDesc, StoreOp, TextureOrigin, TimerScope, Viewport,
};
pub use device::{Device, QueueKind, Submit};
pub use error::RhiError;
pub use format::Format;
pub use handle::{BindlessIndex, Buffer, Heap, Pipeline, Surface, Texture, Timeline};
pub use pipeline::{
    BlendMode, CompareOp, ComputePipelineDesc, CullMode, DepthState, GraphicsPipelineDesc,
    MAX_MESH_PAYLOAD_BYTES, MAX_MESH_STAGE_THREADS, MeshPipelineDesc, MeshStage, ObjectStage,
    ShaderSource, StaticSampler,
};
pub use resource::{BufferDesc, BufferUsage, HeapDesc, MemoryLocation, TextureDesc, TextureUsage};
pub use slots::Slots;
pub use surface::{
    GpuTimeCallback, GpuTiming, Present, PresentedCallback, ScopeTime, SurfaceTarget,
};
