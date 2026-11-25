//! Translates a validated `CommandList` into one Metal command buffer. Hazards are tracked by Metal
//! (tracked resources plus `useResource` for bindless ones), so RHI barriers only end the current encoder.

use concordia_rhi::{
    Buffer, ClearValue, Command, LoadOp, Pipeline, RenderPassDesc, RhiError, StoreOp, Submit,
    Texture, Timeline,
};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLBlitCommandEncoder, MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLComputeCommandEncoder, MTLIndexType, MTLLoadAction, MTLOrigin, MTLPrimitiveType,
    MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLRenderStages, MTLResource,
    MTLResourceUsage, MTLSize, MTLStoreAction, MTLWinding,
};

use crate::bindless::{MtlBuffer, MtlTexture};
use crate::device::{Inner, PipelineEntry, as_event};
use crate::slots;
use crate::timing::TimerPlan;

type CommandBuffer = ProtocolObject<dyn MTLCommandBuffer>;
type ComputeEncoder = ProtocolObject<dyn MTLComputeCommandEncoder>;
type RenderEncoder = ProtocolObject<dyn MTLRenderCommandEncoder>;
type BlitEncoder = ProtocolObject<dyn MTLBlitCommandEncoder>;

enum Encoder {
    None,
    Compute(Retained<ComputeEncoder>, MTLSize),
    Render(Retained<RenderEncoder>),
    Blit(Retained<BlitEncoder>),
}

impl Encoder {
    fn end(&mut self) {
        match std::mem::replace(self, Self::None) {
            Self::None => {}
            Self::Compute(e, _) => e.endEncoding(),
            Self::Render(e) => e.endEncoding(),
            Self::Blit(e) => e.endEncoding(),
        }
    }
}

struct Recorder<'a> {
    inner: &'a Inner,
    cb: Retained<CommandBuffer>,
    encoder: Encoder,
    push: Vec<u8>,
    timer: Option<TimerPlan>,
}

/// A recorded command buffer (not yet committed) and its timer scopes, if any are sampled.
pub(crate) struct Recorded {
    pub(crate) cb: Retained<CommandBuffer>,
    pub(crate) timer: Option<TimerPlan>,
}

/// Records `submit` into a new command buffer; `timed` samples its timer scopes.
pub(crate) fn record(
    inner: &Inner,
    submit: &Submit<'_>,
    timed: bool,
) -> Result<Recorded, RhiError> {
    let cb = inner
        .queue
        .commandBuffer()
        .ok_or_else(|| RhiError::DeviceLost("could not create a command buffer".into()))?;
    for (timeline, value) in submit.wait {
        cb.encodeWaitForEvent_value(as_event(event(inner, *timeline)?), *value);
    }
    let mut recorder = Recorder {
        inner,
        cb,
        encoder: Encoder::None,
        push: Vec::new(),
        timer: TimerPlan::for_list(&inner.device, submit.commands, timed),
    };
    for command in &submit.commands.commands {
        if let Err(e) = recorder.command(command) {
            // Metal aborts if an encoder is released without ending it.
            recorder.encoder.end();
            return Err(e);
        }
    }
    recorder.encoder.end();
    for (timeline, value) in submit.signal {
        recorder
            .cb
            .encodeSignalEvent_value(as_event(event(inner, *timeline)?), *value);
    }
    Ok(Recorded {
        cb: recorder.cb,
        timer: recorder.timer,
    })
}

impl Recorder<'_> {
    fn command(&mut self, command: &Command) -> Result<(), RhiError> {
        match command {
            Command::Barrier { .. } => self.encoder.end(),
            Command::BeginTimer(scope) => {
                if let Some(timer) = &mut self.timer {
                    self.encoder.end();
                    timer.begin(*scope);
                }
            }
            Command::EndTimer => {
                if let Some(timer) = &mut self.timer {
                    self.encoder.end();
                    timer.end();
                }
            }
            Command::BeginRenderPass(desc) => self.begin_pass(desc)?,
            Command::EndRenderPass => self.encoder.end(),
            Command::SetPipeline(pipeline) => self.set_pipeline(*pipeline)?,
            Command::PushConstants(bytes) => {
                self.push.clone_from(bytes);
                self.apply_push();
            }
            Command::Draw {
                vertices,
                instances,
            } => self.draw(vertices.clone(), instances.clone()),
            Command::DrawIndexed {
                indices,
                offset,
                count,
                instances,
            } => self.draw_indexed(*indices, *offset, *count, instances.clone())?,
            Command::Dispatch { groups } => self.dispatch(*groups)?,
            Command::DispatchIndirect { buffer, offset } => {
                self.dispatch_indirect(*buffer, *offset)?
            }
            Command::CopyBuffer {
                src,
                src_offset,
                dst,
                dst_offset,
                size,
            } => {
                let (s, d) = (buffer(self.inner, *src)?, buffer(self.inner, *dst)?);
                let blit = self.blit();
                // SAFETY: Metal validates ranges in debug; handles were checked to be live.
                unsafe {
                    blit.copyFromBuffer_sourceOffset_toBuffer_destinationOffset_size(
                        s,
                        *src_offset as usize,
                        d,
                        *dst_offset as usize,
                        *size as usize,
                    );
                }
            }
            Command::CopyBufferToTexture {
                src,
                offset,
                bytes_per_row,
                dst,
                mip,
            } => {
                let (s, (t, size)) = (buffer(self.inner, *src)?, texture(self.inner, *dst, *mip)?);
                let rows = *bytes_per_row as usize * size.height;
                let blit = self.blit();
                // SAFETY: as above; the copy covers the mip's full extent.
                unsafe {
                    blit.copyFromBuffer_sourceOffset_sourceBytesPerRow_sourceBytesPerImage_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                        s, *offset as usize, *bytes_per_row as usize, rows, size, t, 0, *mip as usize, MTLOrigin { x: 0, y: 0, z: 0 },
                    );
                }
            }
            Command::DrawIndirect { args, offset } => self.draw_indirect(None, *args, *offset)?,
            Command::DrawIndexedIndirect {
                indices,
                args,
                offset,
            } => self.draw_indirect(Some(*indices), *args, *offset)?,
            Command::SetViewport(_) | Command::SetScissor(_) | Command::CopyTexture { .. } => {
                return Err(RhiError::Unsupported(format!("{command:?} on Metal")));
            }
            Command::CopyTextureToBuffer {
                src,
                mip,
                dst,
                offset,
                bytes_per_row,
            } => {
                let ((t, size), d) = (texture(self.inner, *src, *mip)?, buffer(self.inner, *dst)?);
                let rows = *bytes_per_row as usize * size.height;
                let blit = self.blit();
                // SAFETY: as above; the copy covers the mip's full extent.
                unsafe {
                    blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage(
                        t, 0, *mip as usize, MTLOrigin { x: 0, y: 0, z: 0 }, size, d, *offset as usize, *bytes_per_row as usize, rows,
                    );
                }
            }
        }
        Ok(())
    }

    fn blit(&mut self) -> Retained<BlitEncoder> {
        if let Encoder::Blit(e) = &self.encoder {
            return e.clone();
        }
        self.encoder.end();
        let e = match self.timer.as_mut().and_then(TimerPlan::blit_pass) {
            Some(desc) => self.cb.blitCommandEncoderWithDescriptor(&desc),
            None => self.cb.blitCommandEncoder(),
        }
        .expect("Metal returns a blit encoder for a fresh command buffer");
        self.encoder = Encoder::Blit(e.clone());
        e
    }

    fn compute(&mut self) -> (Retained<ComputeEncoder>, MTLSize) {
        if let Encoder::Compute(e, threads) = &self.encoder {
            return (e.clone(), *threads);
        }
        self.encoder.end();
        let e = match self.timer.as_mut().and_then(TimerPlan::compute_pass) {
            Some(desc) => self.cb.computeCommandEncoderWithDescriptor(&desc),
            None => self.cb.computeCommandEncoder(),
        }
        .expect("Metal returns a compute encoder for a fresh command buffer");
        // SAFETY: the tables are live shared buffers owned by the device.
        unsafe {
            e.setBuffer_offset_atIndex(Some(self.inner.buffer_table.raw()), 0, slots::BUFFERS);
            e.setBuffer_offset_atIndex(Some(self.inner.texture_table.raw()), 0, slots::TEXTURES);
            e.setBuffer_offset_atIndex(
                Some(self.inner.texture_table.raw()),
                0,
                slots::STORAGE_TEXTURES,
            );
        }
        for resource in bindless_resources(self.inner) {
            e.useResource_usage(resource, MTLResourceUsage::Read | MTLResourceUsage::Write);
        }
        let threads = MTLSize {
            width: 1,
            height: 1,
            depth: 1,
        };
        self.encoder = Encoder::Compute(e.clone(), threads);
        self.apply_push();
        (e, threads)
    }

    fn begin_pass(&mut self, desc: &RenderPassDesc) -> Result<(), RhiError> {
        self.encoder.end();
        let rp = MTLRenderPassDescriptor::renderPassDescriptor();
        for (i, color) in desc.colors.iter().enumerate() {
            // SAFETY: at most 8 attachments (validated).
            let a = unsafe { rp.colorAttachments().objectAtIndexedSubscript(i) };
            a.setTexture(Some(texture(self.inner, color.texture, 0)?.0));
            if let Some(resolve) = color.resolve {
                a.setResolveTexture(Some(texture(self.inner, resolve, 0)?.0));
            }
            let (load, clear) = load_action(color.load);
            a.setLoadAction(load);
            if let Some(ClearValue::Color([r, g, b, alpha])) = clear {
                a.setClearColor(MTLClearColor {
                    red: r.into(),
                    green: g.into(),
                    blue: b.into(),
                    alpha: alpha.into(),
                });
            }
            a.setStoreAction(match (color.resolve.is_some(), color.store) {
                (true, StoreOp::Store) => MTLStoreAction::StoreAndMultisampleResolve,
                (true, StoreOp::DontCare) => MTLStoreAction::MultisampleResolve,
                (false, StoreOp::Store) => MTLStoreAction::Store,
                (false, StoreOp::DontCare) => MTLStoreAction::DontCare,
            });
        }
        if let Some(depth) = &desc.depth {
            let a = rp.depthAttachment();
            a.setTexture(Some(texture(self.inner, depth.texture, 0)?.0));
            let (load, clear) = load_action(depth.load);
            a.setLoadAction(load);
            if let Some(ClearValue::Depth(d)) = clear {
                a.setClearDepth(d.into());
            }
            a.setStoreAction(match depth.store {
                StoreOp::Store => MTLStoreAction::Store,
                StoreOp::DontCare => MTLStoreAction::DontCare,
            });
        }
        if let Some(timer) = &mut self.timer {
            timer.attach_render(&rp);
        }
        let e = self
            .cb
            .renderCommandEncoderWithDescriptor(&rp)
            .ok_or_else(|| RhiError::invalid("render pass", format!("Metal rejected {desc:?}")))?;
        e.setFrontFacingWinding(MTLWinding::CounterClockwise);
        // SAFETY: the tables are live shared buffers owned by the device.
        unsafe {
            for slot in [slots::BUFFERS, slots::TEXTURES, slots::STORAGE_TEXTURES] {
                let table = if slot == slots::BUFFERS {
                    self.inner.buffer_table.raw()
                } else {
                    self.inner.texture_table.raw()
                };
                e.setVertexBuffer_offset_atIndex(Some(table), 0, slot);
                e.setFragmentBuffer_offset_atIndex(Some(table), 0, slot);
            }
        }
        for resource in bindless_resources(self.inner) {
            e.useResource_usage_stages(
                resource,
                MTLResourceUsage::Read | MTLResourceUsage::Write,
                MTLRenderStages::Vertex | MTLRenderStages::Fragment,
            );
        }
        self.encoder = Encoder::Render(e);
        self.apply_push();
        Ok(())
    }

    fn set_pipeline(&mut self, pipeline: Pipeline) -> Result<(), RhiError> {
        let entry = self
            .inner
            .pipelines
            .get(pipeline.index(), pipeline.generation())
            .ok_or_else(|| RhiError::StaleHandle {
                kind: "pipeline",
                index: pipeline.index(),
                generation: pipeline.generation(),
            })?;
        match (entry, &self.encoder) {
            (PipelineEntry::Graphics { state, depth, cull }, Encoder::Render(e)) => {
                e.setRenderPipelineState(state);
                if let Some(depth) = depth {
                    e.setDepthStencilState(Some(depth));
                }
                e.setCullMode(*cull);
                Ok(())
            }
            (PipelineEntry::Graphics { .. }, _) => Err(RhiError::invalid(
                "command list",
                "graphics pipeline set outside a render pass",
            )),
            (PipelineEntry::Compute { state, threads }, _) => {
                let (e, _) = self.compute();
                e.setComputePipelineState(state);
                self.encoder = Encoder::Compute(e, *threads);
                Ok(())
            }
        }
    }

    fn apply_push(&self) {
        if self.push.is_empty() {
            return;
        }
        let ptr = std::ptr::NonNull::new(self.push.as_ptr().cast_mut().cast())
            .expect("Vec data is non-null");
        // SAFETY: setBytes copies `len` bytes from a live Vec during the call.
        unsafe {
            match &self.encoder {
                Encoder::Compute(e, _) => {
                    e.setBytes_length_atIndex(ptr, self.push.len(), slots::PUSH)
                }
                Encoder::Render(e) => {
                    e.setVertexBytes_length_atIndex(ptr, self.push.len(), slots::PUSH);
                    e.setFragmentBytes_length_atIndex(ptr, self.push.len(), slots::PUSH);
                }
                Encoder::Blit(_) | Encoder::None => {}
            }
        }
    }

    fn draw(&self, vertices: std::ops::Range<u32>, instances: std::ops::Range<u32>) {
        if let Encoder::Render(e) = &self.encoder {
            // SAFETY: counts come from validated ranges; the pipeline pulls vertices itself.
            unsafe {
                e.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                    MTLPrimitiveType::Triangle,
                    vertices.start as usize,
                    vertices.len(),
                    instances.len(),
                    instances.start as usize,
                );
            }
        }
    }

    fn draw_indexed(
        &self,
        indices: Buffer,
        offset: u64,
        count: u32,
        instances: std::ops::Range<u32>,
    ) -> Result<(), RhiError> {
        let index_buffer = buffer(self.inner, indices)?;
        if let Encoder::Render(e) = &self.encoder {
            // SAFETY: the index buffer is live; Metal validates the range in debug builds.
            unsafe {
                e.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset_instanceCount_baseVertex_baseInstance(
                    MTLPrimitiveType::Triangle,
                    count as usize,
                    MTLIndexType::UInt32,
                    index_buffer,
                    offset as usize,
                    instances.len(),
                    0,
                    instances.start as usize,
                );
            }
        }
        Ok(())
    }

    /// One indirect draw, indexed when `indices` is given; the argument range is checked first.
    fn draw_indirect(
        &self,
        indices: Option<Buffer>,
        args: Buffer,
        offset: u64,
    ) -> Result<(), RhiError> {
        let bytes = match indices {
            Some(_) => concordia_rhi::DRAW_INDEXED_INDIRECT_BYTES,
            None => concordia_rhi::DRAW_INDIRECT_BYTES,
        };
        let entry = buffer_entry(self.inner, args)?;
        concordia_rhi::check::indirect_args(&entry.desc, offset, bytes)?;
        let index_buffer = indices.map(|i| buffer(self.inner, i)).transpose()?;
        let Encoder::Render(e) = &self.encoder else {
            return Ok(());
        };
        match index_buffer {
            // SAFETY: both buffers are live and the argument record lies inside `args` (checked);
            // Metal tracks the hazard on `args` against earlier GPU writes.
            Some(index_buffer) => unsafe {
                e.drawIndexedPrimitives_indexType_indexBuffer_indexBufferOffset_indirectBuffer_indirectBufferOffset(
                    MTLPrimitiveType::Triangle,
                    MTLIndexType::UInt32,
                    index_buffer,
                    0,
                    &entry.raw,
                    offset as usize,
                )
            },
            // SAFETY: as above; the pipeline pulls its vertices itself.
            None => unsafe {
                e.drawPrimitives_indirectBuffer_indirectBufferOffset(
                    MTLPrimitiveType::Triangle,
                    &entry.raw,
                    offset as usize,
                )
            },
        }
        Ok(())
    }

    fn dispatch(&mut self, groups: [u32; 3]) -> Result<(), RhiError> {
        let Encoder::Compute(e, threads) = &self.encoder else {
            return Err(RhiError::invalid(
                "command list",
                "Dispatch without a compute pipeline",
            ));
        };
        let size = MTLSize {
            width: groups[0] as usize,
            height: groups[1] as usize,
            depth: groups[2] as usize,
        };
        e.dispatchThreadgroups_threadsPerThreadgroup(size, *threads);
        Ok(())
    }

    fn dispatch_indirect(&mut self, args: Buffer, offset: u64) -> Result<(), RhiError> {
        let raw = buffer(self.inner, args)?;
        let Encoder::Compute(e, threads) = &self.encoder else {
            return Err(RhiError::invalid(
                "command list",
                "DispatchIndirect without a compute pipeline",
            ));
        };
        // SAFETY: the argument buffer is live and holds three u32 group counts at `offset`.
        unsafe {
            e.dispatchThreadgroupsWithIndirectBuffer_indirectBufferOffset_threadsPerThreadgroup(
                raw,
                offset as usize,
                *threads,
            )
        };
        Ok(())
    }
}

fn load_action(load: LoadOp) -> (MTLLoadAction, Option<ClearValue>) {
    match load {
        LoadOp::Load => (MTLLoadAction::Load, None),
        LoadOp::Clear(value) => (MTLLoadAction::Clear, Some(value)),
        LoadOp::DontCare => (MTLLoadAction::DontCare, None),
    }
}

fn buffer(inner: &Inner, handle: Buffer) -> Result<&MtlBuffer, RhiError> {
    buffer_entry(inner, handle).map(|e| &*e.raw)
}

fn buffer_entry(inner: &Inner, handle: Buffer) -> Result<&crate::device::BufferEntry, RhiError> {
    inner
        .buffers
        .get(handle.index(), handle.generation())
        .ok_or_else(|| RhiError::StaleHandle {
            kind: "buffer",
            index: handle.index(),
            generation: handle.generation(),
        })
}

/// The texture and the size of one of its mips.
fn texture(inner: &Inner, handle: Texture, mip: u32) -> Result<(&MtlTexture, MTLSize), RhiError> {
    let entry = inner
        .textures
        .get(handle.index(), handle.generation())
        .ok_or_else(|| RhiError::StaleHandle {
            kind: "texture",
            index: handle.index(),
            generation: handle.generation(),
        })?;
    if mip >= entry.desc.mip_levels.get() {
        return Err(RhiError::invalid(
            "copy",
            format!("mip {mip} of {:?}", entry.desc.label),
        ));
    }
    let size = MTLSize {
        width: (entry.desc.width.get() >> mip).max(1) as usize,
        height: (entry.desc.height.get() >> mip).max(1) as usize,
        depth: 1,
    };
    Ok((&entry.raw, size))
}

fn event(inner: &Inner, timeline: Timeline) -> Result<&crate::device::MtlEvent, RhiError> {
    inner
        .timelines
        .get(timeline.index(), timeline.generation())
        .map(|e| &**e)
        .ok_or_else(|| RhiError::StaleHandle {
            kind: "timeline",
            index: timeline.index(),
            generation: timeline.generation(),
        })
}

/// Every resource reachable through the bindless tables.
fn bindless_resources(inner: &Inner) -> impl Iterator<Item = &ProtocolObject<dyn MTLResource>> {
    let buffers = inner
        .buffers
        .values()
        .filter(|b| b.bindless.is_some())
        .map(|b| ProtocolObject::from_ref(&*b.raw));
    let textures = inner
        .textures
        .values()
        .filter(|t| t.bindless.is_some())
        .map(|t| ProtocolObject::from_ref(&*t.raw));
    buffers.chain(textures)
}
