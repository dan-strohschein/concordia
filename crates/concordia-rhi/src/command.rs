//! Commands recorded as data and translated by a backend at submit.

use std::ops::Range;

use crate::barrier::{BufferBarrier, TextureBarrier};
use crate::error::RhiError;
use crate::handle::{Buffer, Pipeline, Texture};

/// Largest push-constant block in bytes.
pub const MAX_PUSH_CONSTANTS: usize = 128;

/// A clear value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClearValue {
    /// RGBA color.
    Color([f32; 4]),
    /// Depth (0 is far with reverse-Z).
    Depth(f32),
}

/// What happens to an attachment at the start of a pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoadOp {
    /// Keep previous contents.
    Load,
    /// Clear to a value.
    Clear(ClearValue),
    /// Contents are undefined.
    DontCare,
}

/// What happens to an attachment at the end of a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreOp {
    /// Keep the results.
    Store,
    /// Discard (e.g. an MSAA target that was resolved).
    DontCare,
}

/// A color attachment, optionally resolved (MSAA) into a single-sample texture.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorAttachment {
    /// The target.
    pub texture: Texture,
    /// Single-sample resolve target.
    pub resolve: Option<Texture>,
    /// Load action.
    pub load: LoadOp,
    /// Store action.
    pub store: StoreOp,
}

/// A depth attachment.
#[derive(Debug, Clone, PartialEq)]
pub struct DepthAttachment {
    /// The depth texture.
    pub texture: Texture,
    /// Load action.
    pub load: LoadOp,
    /// Store action.
    pub store: StoreOp,
}

/// A render pass's attachments.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderPassDesc {
    /// Color attachments (1..=8).
    pub colors: Vec<ColorAttachment>,
    /// Optional depth attachment.
    pub depth: Option<DepthAttachment>,
}

/// One recorded command.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Dependencies between earlier and later work.
    Barrier {
        /// Buffer dependencies.
        buffers: Vec<BufferBarrier>,
        /// Texture dependencies.
        textures: Vec<TextureBarrier>,
    },
    /// Copy bytes between buffers.
    CopyBuffer {
        /// Source.
        src: Buffer,
        /// Source offset.
        src_offset: u64,
        /// Destination.
        dst: Buffer,
        /// Destination offset.
        dst_offset: u64,
        /// Bytes.
        size: u64,
    },
    /// Copy tightly packed rows from a buffer into a texture mip.
    CopyBufferToTexture {
        /// Source.
        src: Buffer,
        /// Source offset.
        offset: u64,
        /// Bytes per row in the buffer.
        bytes_per_row: u32,
        /// Destination.
        dst: Texture,
        /// Destination mip.
        mip: u32,
    },
    /// Copy a texture mip into a buffer as rows of `bytes_per_row`.
    CopyTextureToBuffer {
        /// Source.
        src: Texture,
        /// Source mip.
        mip: u32,
        /// Destination.
        dst: Buffer,
        /// Destination offset.
        offset: u64,
        /// Bytes per row in the buffer.
        bytes_per_row: u32,
    },
    /// Begin a render pass.
    BeginRenderPass(RenderPassDesc),
    /// End the current render pass.
    EndRenderPass,
    /// Bind a pipeline (graphics inside a pass, compute outside).
    SetPipeline(Pipeline),
    /// Set the push-constant block (≤ [`MAX_PUSH_CONSTANTS`] bytes) for following draws/dispatches.
    PushConstants(Vec<u8>),
    /// Draw without an index buffer (vertex pulling).
    Draw {
        /// Vertex range.
        vertices: Range<u32>,
        /// Instance range.
        instances: Range<u32>,
    },
    /// Draw with 32-bit indices.
    DrawIndexed {
        /// Index buffer.
        indices: Buffer,
        /// Byte offset of the first index.
        offset: u64,
        /// Index count.
        count: u32,
        /// Instance range.
        instances: Range<u32>,
    },
    /// Dispatch compute thread groups.
    Dispatch {
        /// Group counts.
        groups: [u32; 3],
    },
    /// Dispatch with group counts read from a buffer (three `u32`).
    DispatchIndirect {
        /// Argument buffer.
        buffer: Buffer,
        /// Byte offset.
        offset: u64,
    },
}

/// A labelled sequence of commands.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommandList {
    /// Debug label.
    pub label: String,
    /// Commands in order.
    pub commands: Vec<Command>,
}

impl CommandList {
    /// An empty list.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            commands: Vec::new(),
        }
    }

    /// Appends a command.
    pub fn push(&mut self, command: Command) -> &mut Self {
        self.commands.push(command);
        self
    }

    /// Checks structure: draws only inside passes, dispatches/copies/barriers outside, passes closed, push constants small.
    pub fn validate(&self) -> Result<(), RhiError> {
        let mut in_pass = false;
        for (i, command) in self.commands.iter().enumerate() {
            let fail = |reason: &str| {
                Err(RhiError::invalid(
                    "command list",
                    format!("{:?} command {i}: {reason}", self.label),
                ))
            };
            match command {
                Command::BeginRenderPass(desc) if in_pass => {
                    return fail(&format!("nested render pass {desc:?}"));
                }
                Command::BeginRenderPass(desc)
                    if desc.colors.is_empty() && desc.depth.is_none() =>
                {
                    return fail("render pass without attachments");
                }
                Command::BeginRenderPass(desc) if desc.colors.len() > 8 => {
                    return fail("more than 8 color attachments");
                }
                Command::BeginRenderPass(_) => in_pass = true,
                Command::EndRenderPass if !in_pass => {
                    return fail("EndRenderPass without a render pass");
                }
                Command::EndRenderPass => in_pass = false,
                Command::Draw { .. } | Command::DrawIndexed { .. } if !in_pass => {
                    return fail("draws must be inside a render pass");
                }
                Command::Dispatch { .. }
                | Command::DispatchIndirect { .. }
                | Command::CopyBuffer { .. }
                | Command::CopyBufferToTexture { .. }
                | Command::CopyTextureToBuffer { .. }
                | Command::Barrier { .. }
                    if in_pass =>
                {
                    return fail("dispatches, copies and barriers must be outside render passes");
                }
                Command::PushConstants(bytes) if bytes.len() > MAX_PUSH_CONSTANTS => {
                    return fail(&format!(
                        "{} push-constant bytes; the limit is {MAX_PUSH_CONSTANTS}",
                        bytes.len()
                    ));
                }
                _ => {}
            }
        }
        match in_pass {
            true => Err(RhiError::invalid(
                "command list",
                format!("{:?}: render pass not ended", self.label),
            )),
            false => Ok(()),
        }
    }
}
