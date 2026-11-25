//! Commands recorded as data and translated by a backend at submit.

use std::num::NonZeroU32;
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

/// Bytes of one [`Command::DrawIndirect`] record: vertex count, instance count, first vertex, first
/// instance (`u32` each; Metal and Vulkan share the layout).
pub const DRAW_INDIRECT_BYTES: u64 = 16;
/// Bytes of one [`Command::DrawIndexedIndirect`] record: index count, instance count, first index,
/// vertex offset (`i32`), first instance.
pub const DRAW_INDEXED_INDIRECT_BYTES: u64 = 20;

/// Most mesh-task groups per dimension of one draw on every backend (Vulkan's minimum).
pub const MAX_MESH_TASK_GROUPS: u32 = 65_535;
/// Most mesh-task groups in one draw on every backend (Vulkan's minimum).
pub const MAX_MESH_TASK_GROUPS_TOTAL: u64 = 1 << 22;

/// The region of the targets a draw maps clip space into, in pixels (y down from the top-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width (> 0).
    pub width: f32,
    /// Height (> 0).
    pub height: f32,
    /// Depth that clip z = 0 maps to (0..=1).
    pub min_depth: f32,
    /// Depth that clip z = 1 maps to (`min_depth..=1`).
    pub max_depth: f32,
}

impl Viewport {
    /// Finite, non-negative corner, positive size, depths ordered within 0..=1.
    fn validate(&self) -> Result<(), String> {
        let finite = [
            self.x,
            self.y,
            self.width,
            self.height,
            self.min_depth,
            self.max_depth,
        ]
        .iter()
        .all(|v| v.is_finite());
        let ok = finite
            && self.x >= 0.0
            && self.y >= 0.0
            && self.width > 0.0
            && self.height > 0.0
            && (0.0..=1.0).contains(&self.min_depth)
            && (self.min_depth..=1.0).contains(&self.max_depth);
        match ok {
            true => Ok(()),
            false => Err(format!("invalid viewport {self:?}")),
        }
    }
}

/// A pixel rectangle (y down from the top-left).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub width: NonZeroU32,
    /// Height.
    pub height: NonZeroU32,
}

/// A texel of one mip, the corner of a copied region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextureOrigin {
    /// Mip level.
    pub mip: u32,
    /// Column.
    pub x: u32,
    /// Row.
    pub y: u32,
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
    /// One draw (vertex pulling) whose [`DRAW_INDIRECT_BYTES`] arguments are read from `args` at
    /// draw time; `args` needs `BufferUsage::INDIRECT` and `offset` is a multiple of 4.
    DrawIndirect {
        /// Argument buffer.
        args: Buffer,
        /// Byte offset of the record.
        offset: u64,
    },
    /// One draw with 32-bit indices whose [`DRAW_INDEXED_INDIRECT_BYTES`] arguments are read from
    /// `args` at draw time.
    DrawIndexedIndirect {
        /// Index buffer.
        indices: Buffer,
        /// Argument buffer.
        args: Buffer,
        /// Byte offset of the record.
        offset: u64,
    },
    /// Maps clip space into a region of the pass's targets for the following draws (each pass
    /// starts with the whole target).
    SetViewport(Viewport),
    /// Discards fragments outside a rectangle for the following draws (each pass starts with the
    /// whole target).
    SetScissor(Rect),
    /// Copies a region between single-sample textures of the same format (not depth).
    CopyTexture {
        /// Source.
        src: Texture,
        /// Source corner and mip.
        src_origin: TextureOrigin,
        /// Destination.
        dst: Texture,
        /// Destination corner and mip.
        dst_origin: TextureOrigin,
        /// Width and height of the region.
        extent: [NonZeroU32; 2],
    },
    /// Draws `groups` object groups (or mesh groups, without an object stage) with the bound mesh
    /// pipeline; each count is 1..=[`MAX_MESH_TASK_GROUPS`] and their product at most
    /// [`MAX_MESH_TASK_GROUPS_TOTAL`].
    DrawMeshTasks {
        /// Group counts.
        groups: [u32; 3],
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
    /// Start timing the commands up to the next [`Command::EndTimer`] (outside render passes; no
    /// nesting). Backends without per-pass timestamps ignore it (`RhiCaps::timestamp_granularity`).
    BeginTimer(TimerScope),
    /// Stop the open timer.
    EndTimer,
}

/// Names one timed range within a command list (unique per list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimerScope(pub u32);

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
        let mut state = Structure::default();
        for (i, command) in self.commands.iter().enumerate() {
            state.step(command).map_err(|reason| {
                RhiError::invalid(
                    "command list",
                    format!("{:?} command {i}: {reason}", self.label),
                )
            })?;
        }
        state.finish().map_err(|reason| {
            RhiError::invalid("command list", format!("{:?}: {reason}", self.label))
        })
    }
}

/// Each count is 1..=[`MAX_MESH_TASK_GROUPS`] and the product at most [`MAX_MESH_TASK_GROUPS_TOTAL`].
fn mesh_task_groups(groups: [u32; 3]) -> Result<(), String> {
    let each = groups
        .iter()
        .all(|g| (1..=MAX_MESH_TASK_GROUPS).contains(g));
    let total = groups.iter().map(|&g| u64::from(g)).product::<u64>();
    match each && total <= MAX_MESH_TASK_GROUPS_TOTAL {
        true => Ok(()),
        false => Err(format!(
            "mesh-task groups {groups:?}: each 1..={MAX_MESH_TASK_GROUPS}, at most \
             {MAX_MESH_TASK_GROUPS_TOTAL} in total"
        )),
    }
}

/// Where validation is in a command list: inside a render pass, inside a timer, scopes seen.
#[derive(Default)]
struct Structure {
    in_pass: bool,
    timer: Option<TimerScope>,
    scopes: std::collections::BTreeSet<TimerScope>,
}

impl Structure {
    fn step(&mut self, command: &Command) -> Result<(), String> {
        match command {
            Command::BeginTimer(_) | Command::EndTimer => self.timer_step(command),
            _ => self.pass_step(command),
        }
    }

    fn pass_step(&mut self, command: &Command) -> Result<(), String> {
        match command {
            Command::BeginRenderPass(desc) if self.in_pass => {
                Err(format!("nested render pass {desc:?}"))
            }
            Command::BeginRenderPass(desc) if desc.colors.is_empty() && desc.depth.is_none() => {
                Err("render pass without attachments".into())
            }
            Command::BeginRenderPass(desc) if desc.colors.len() > 8 => {
                Err("more than 8 color attachments".into())
            }
            Command::BeginRenderPass(_) => {
                self.in_pass = true;
                Ok(())
            }
            Command::EndRenderPass if !self.in_pass => {
                Err("EndRenderPass without a render pass".into())
            }
            Command::EndRenderPass => {
                self.in_pass = false;
                Ok(())
            }
            Command::Draw { .. }
            | Command::DrawIndexed { .. }
            | Command::DrawIndirect { .. }
            | Command::DrawIndexedIndirect { .. }
            | Command::DrawMeshTasks { .. }
                if !self.in_pass =>
            {
                Err("draws must be inside a render pass".into())
            }
            Command::SetViewport(_) | Command::SetScissor(_) if !self.in_pass => {
                Err("viewports and scissors are set inside a render pass".into())
            }
            Command::DrawIndirect { offset, .. } | Command::DrawIndexedIndirect { offset, .. }
                if offset % 4 != 0 =>
            {
                Err(format!(
                    "indirect argument offset {offset} is not a multiple of 4"
                ))
            }
            Command::SetViewport(viewport) => viewport.validate(),
            Command::DrawMeshTasks { groups } => mesh_task_groups(*groups),
            Command::Dispatch { .. }
            | Command::DispatchIndirect { .. }
            | Command::CopyBuffer { .. }
            | Command::CopyBufferToTexture { .. }
            | Command::CopyTextureToBuffer { .. }
            | Command::CopyTexture { .. }
            | Command::Barrier { .. }
                if self.in_pass =>
            {
                Err("dispatches, copies and barriers must be outside render passes".into())
            }
            Command::PushConstants(bytes) if bytes.len() > MAX_PUSH_CONSTANTS => Err(format!(
                "{} push-constant bytes; the limit is {MAX_PUSH_CONSTANTS}",
                bytes.len()
            )),
            _ => Ok(()),
        }
    }

    fn timer_step(&mut self, command: &Command) -> Result<(), String> {
        if self.in_pass {
            return Err("timers must begin and end outside render passes".into());
        }
        match command {
            Command::BeginTimer(scope) => self.begin_timer(*scope),
            _ => self.end_timer(),
        }
    }

    fn begin_timer(&mut self, scope: TimerScope) -> Result<(), String> {
        if let Some(open) = self.timer {
            return Err(format!(
                "timer {scope:?} begins inside timer {open:?} (no nesting)"
            ));
        }
        if !self.scopes.insert(scope) {
            return Err(format!("timer {scope:?} is used twice"));
        }
        self.timer = Some(scope);
        Ok(())
    }

    fn end_timer(&mut self) -> Result<(), String> {
        self.timer
            .take()
            .map(|_| ())
            .ok_or_else(|| "EndTimer without an open timer".into())
    }

    fn finish(&self) -> Result<(), String> {
        match (self.in_pass, self.timer) {
            (true, _) => Err("render pass not ended".into()),
            (_, Some(scope)) => Err(format!("timer {scope:?} not ended")),
            _ => Ok(()),
        }
    }
}
