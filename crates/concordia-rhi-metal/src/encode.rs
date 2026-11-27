//! Translates a validated `CommandList` into one Metal command buffer. Hazards are tracked by Metal
//! (tracked resources plus `useResource` for bindless ones), so RHI barriers only end the current encoder.

use std::num::NonZeroU32;

use concordia_rhi::{
    Buffer, ClearValue, Command, DepthResolve, LoadOp, Pipeline, Rect, RenderPassDesc, RhiCaps,
    RhiError, StoreOp, Submit, Texture, TextureOrigin, Timeline, Viewport, check,
};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLBlitCommandEncoder, MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLComputeCommandEncoder, MTLCullMode, MTLDepthStencilState, MTLIndexType, MTLLoadAction,
    MTLMultisampleDepthResolveFilter, MTLOrigin, MTLPrimitiveType, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLRenderPipelineState, MTLRenderStages, MTLResource,
    MTLResourceUsage, MTLScissorRect, MTLSize, MTLStoreAction, MTLViewport, MTLWinding,
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
    /// The open render pass's target size (width, height).
    pass_size: Option<[NonZeroU32; 2]>,
    /// What kind of pipeline the open render pass has bound.
    bound: Bound,
    /// The device has object and mesh stages (their tables and push constants are bound).
    mesh_stages: bool,
    /// The device resolves depth with the min filter.
    depth_resolve_min: bool,
    /// `RhiCaps::max_render_extent`, checked against attachment-less passes.
    max_render_extent: u32,
}

/// The pipeline bound in a render pass: draws must match it.
#[derive(Debug, Clone, Copy)]
enum Bound {
    None,
    Vertex,
    Mesh { object: MTLSize, mesh: MTLSize },
}

/// A recorded command buffer (not yet committed) and its timer scopes, if any are sampled.
pub(crate) struct Recorded {
    pub(crate) cb: Retained<CommandBuffer>,
    pub(crate) timer: Option<TimerPlan>,
}

/// Records `submit` into a new command buffer for a device with `caps`; `timed` samples its timer
/// scopes.
pub(crate) fn record(
    inner: &Inner,
    submit: &Submit<'_>,
    timed: bool,
    caps: &RhiCaps,
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
        pass_size: None,
        bound: Bound::None,
        mesh_stages: caps.mesh_shaders,
        depth_resolve_min: caps.depth_resolve_min,
        max_render_extent: caps.max_render_extent,
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
            Command::TemporalScale(scale) => self.temporal_scale(scale)?,
            Command::EndRenderPass => {
                self.pass_size = None;
                self.bound = Bound::None;
                self.encoder.end();
            }
            Command::SetPipeline(pipeline) => self.set_pipeline(*pipeline)?,
            Command::PushConstants(bytes) => {
                self.push.clone_from(bytes);
                self.apply_push();
            }
            Command::Draw {
                vertices,
                instances,
            } => {
                self.vertex_bound()?;
                self.draw(vertices.clone(), instances.clone())
            }
            Command::DrawIndexed {
                indices,
                offset,
                count,
                instances,
            } => {
                self.vertex_bound()?;
                self.draw_indexed(*indices, *offset, *count, instances.clone())?
            }
            Command::Dispatch { groups } => self.dispatch(*groups)?,
            Command::DrawMeshTasks { groups } => self.draw_mesh_tasks(*groups)?,
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
            Command::DrawIndirect { args, offset } => {
                self.vertex_bound()?;
                self.draw_indirect(None, *args, *offset)?
            }
            Command::DrawIndexedIndirect {
                indices,
                args,
                offset,
            } => {
                self.vertex_bound()?;
                self.draw_indirect(Some(*indices), *args, *offset)?
            }
            Command::SetViewport(viewport) => self.set_viewport(viewport)?,
            Command::SetScissor(rect) => self.set_scissor(rect)?,
            Command::CopyTexture {
                src,
                src_origin,
                dst,
                dst_origin,
                extent,
            } => self.copy_texture((*src, *src_origin), (*dst, *dst_origin), *extent)?,
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
        let first = desc
            .colors
            .first()
            .map(|c| c.texture)
            .or(desc.depth.as_ref().map(|d| d.texture));
        self.pass_size = first
            .map(|t| texture_entry(self.inner, t).map(|e| [e.desc.width, e.desc.height]))
            .transpose()?
            .or(desc.extent);
        let rp = MTLRenderPassDescriptor::renderPassDescriptor();
        if let Some(extent) = desc.extent {
            concordia_rhi::check::render_extent(extent, self.max_render_extent)?;
            rp.setRenderTargetWidth(extent[0].get() as usize);
            rp.setRenderTargetHeight(extent[1].get() as usize);
            rp.setDefaultRasterSampleCount(1);
        }
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
            if let Some(resolve) = depth.resolve {
                let source = &texture_entry(self.inner, depth.texture)?.desc;
                let target = &texture_entry(self.inner, resolve.texture)?.desc;
                check::depth_resolve(source, target, self.depth_resolve_min)?;
                a.setResolveTexture(Some(texture(self.inner, resolve.texture, 0)?.0));
                a.setDepthResolveFilter(match resolve.filter {
                    DepthResolve::Min => MTLMultisampleDepthResolveFilter::Min,
                });
            }
            a.setStoreAction(match (depth.resolve.is_some(), depth.store) {
                (true, StoreOp::Store) => MTLStoreAction::StoreAndMultisampleResolve,
                (true, StoreOp::DontCare) => MTLStoreAction::MultisampleResolve,
                (false, StoreOp::Store) => MTLStoreAction::Store,
                (false, StoreOp::DontCare) => MTLStoreAction::DontCare,
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
                if self.mesh_stages {
                    e.setObjectBuffer_offset_atIndex(Some(table), 0, slot);
                    e.setMeshBuffer_offset_atIndex(Some(table), 0, slot);
                }
            }
        }
        let stages = match self.mesh_stages {
            true => {
                MTLRenderStages::Vertex
                    | MTLRenderStages::Fragment
                    | MTLRenderStages::Object
                    | MTLRenderStages::Mesh
            }
            false => MTLRenderStages::Vertex | MTLRenderStages::Fragment,
        };
        for resource in bindless_resources(self.inner) {
            e.useResource_usage_stages(
                resource,
                MTLResourceUsage::Read | MTLResourceUsage::Write,
                stages,
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
                set_render_state(e, state, depth.as_deref(), *cull);
                self.bound = Bound::Vertex;
                Ok(())
            }
            (
                PipelineEntry::Mesh {
                    state,
                    depth,
                    cull,
                    object_threads,
                    mesh_threads,
                },
                Encoder::Render(e),
            ) => {
                set_render_state(e, state, depth.as_deref(), *cull);
                self.bound = Bound::Mesh {
                    object: *object_threads,
                    mesh: *mesh_threads,
                };
                Ok(())
            }
            (PipelineEntry::Graphics { .. } | PipelineEntry::Mesh { .. }, _) => {
                Err(RhiError::invalid(
                    "command list",
                    "graphics pipeline set outside a render pass",
                ))
            }
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
                    if self.mesh_stages {
                        e.setObjectBytes_length_atIndex(ptr, self.push.len(), slots::PUSH);
                        e.setMeshBytes_length_atIndex(ptr, self.push.len(), slots::PUSH);
                    }
                }
                Encoder::Blit(_) | Encoder::None => {}
            }
        }
    }

    /// Vertex draws need a vertex pipeline bound in the pass.
    fn vertex_bound(&self) -> Result<(), RhiError> {
        match self.bound {
            Bound::Vertex => Ok(()),
            Bound::None => Err(RhiError::invalid("command list", "draw without a pipeline")),
            Bound::Mesh { .. } => Err(RhiError::invalid(
                "command list",
                "a vertex draw with a mesh pipeline bound",
            )),
        }
    }

    /// `groups` object (or mesh) threadgroups of the bound mesh pipeline.
    fn draw_mesh_tasks(&self, groups: [u32; 3]) -> Result<(), RhiError> {
        let Bound::Mesh { object, mesh } = self.bound else {
            return Err(RhiError::invalid(
                "command list",
                "a mesh-task draw without a mesh pipeline bound",
            ));
        };
        if let Encoder::Render(e) = &self.encoder {
            let [x, y, z] = groups.map(|g| g as usize);
            e.drawMeshThreadgroups_threadsPerObjectThreadgroup_threadsPerMeshThreadgroup(
                MTLSize {
                    width: x,
                    height: y,
                    depth: z,
                },
                object,
                mesh,
            );
        }
        Ok(())
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

    fn pass_size(&self) -> Result<[NonZeroU32; 2], RhiError> {
        self.pass_size
            .ok_or_else(|| RhiError::invalid("command list", "no render pass is open"))
    }

    /// Maps clip space into `viewport` (y down from the top-left, as Metal's).
    fn set_viewport(&self, viewport: &Viewport) -> Result<(), RhiError> {
        concordia_rhi::check::viewport_in(viewport, self.pass_size()?)?;
        if let Encoder::Render(e) = &self.encoder {
            e.setViewport(MTLViewport {
                originX: viewport.x.into(),
                originY: viewport.y.into(),
                width: viewport.width.into(),
                height: viewport.height.into(),
                znear: viewport.min_depth.into(),
                zfar: viewport.max_depth.into(),
            });
        }
        Ok(())
    }

    fn set_scissor(&self, rect: &Rect) -> Result<(), RhiError> {
        concordia_rhi::check::scissor_in(rect, self.pass_size()?)?;
        if let Encoder::Render(e) = &self.encoder {
            e.setScissorRect(MTLScissorRect {
                x: rect.x as usize,
                y: rect.y as usize,
                width: rect.width.get() as usize,
                height: rect.height.get() as usize,
            });
        }
        Ok(())
    }

    /// Copies one region between textures after the shared checks.
    fn copy_texture(
        &mut self,
        (src, from): (Texture, TextureOrigin),
        (dst, to): (Texture, TextureOrigin),
        extent: [NonZeroU32; 2],
    ) -> Result<(), RhiError> {
        let (s, d) = (
            texture_entry(self.inner, src)?,
            texture_entry(self.inner, dst)?,
        );
        let side = |texture, desc, origin| concordia_rhi::check::CopySide {
            texture,
            desc,
            origin,
        };
        concordia_rhi::check::texture_copy(
            side(src, &s.desc, from),
            side(dst, &d.desc, to),
            extent,
        )?;
        let origin = |o: TextureOrigin| MTLOrigin {
            x: o.x as usize,
            y: o.y as usize,
            z: 0,
        };
        let size = MTLSize {
            width: extent[0].get() as usize,
            height: extent[1].get() as usize,
            depth: 1,
        };
        let blit = self.blit();
        // SAFETY: both textures are live, and both regions lie inside their mips (checked).
        unsafe {
            blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                &s.raw, 0, from.mip as usize, origin(from), size, &d.raw, 0, to.mip as usize, origin(to),
            );
        }
        Ok(())
    }

    /// Ends the open encoder and lets MetalFX encode the scale after the shared checks.
    fn temporal_scale(&mut self, scale: &concordia_rhi::TemporalScale) -> Result<(), RhiError> {
        let scaler = self
            .inner
            .scalers
            .get(scale.scaler.index(), scale.scaler.generation())
            .ok_or_else(|| RhiError::StaleHandle {
                kind: "temporal scaler",
                index: scale.scaler.index(),
                generation: scale.scaler.generation(),
            })?;
        let entry = |t| texture_entry(self.inner, t);
        let (color, depth, motion, output) = (
            entry(scale.color)?,
            entry(scale.depth)?,
            entry(scale.motion)?,
            entry(scale.output)?,
        );
        let reactive = scale.reactive.map(entry).transpose()?;
        let textures = check::TemporalTextures {
            color: &color.desc,
            depth: &depth.desc,
            motion: &motion.desc,
            reactive: reactive.map(|r| &r.desc),
            output: &output.desc,
        };
        check::temporal_scale(&scaler.desc, &textures)?;
        let raw = crate::metalfx::Textures {
            color: &color.raw,
            depth: &depth.raw,
            motion: &motion.raw,
            reactive: reactive.map(|r| &*r.raw),
            output: &output.raw,
        };
        crate::metalfx::check_usage(&scaler.raw, &raw)?;
        self.encoder.end();
        crate::metalfx::encode(&scaler.raw, &self.cb, &raw, scale);
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
    let entry = texture_entry(inner, handle)?;
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

fn texture_entry(inner: &Inner, handle: Texture) -> Result<&crate::device::TextureEntry, RhiError> {
    inner
        .textures
        .get(handle.index(), handle.generation())
        .ok_or_else(|| RhiError::StaleHandle {
            kind: "texture",
            index: handle.index(),
            generation: handle.generation(),
        })
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

/// Binds a render pipeline with its depth state and culling.
fn set_render_state(
    e: &RenderEncoder,
    state: &ProtocolObject<dyn MTLRenderPipelineState>,
    depth: Option<&ProtocolObject<dyn MTLDepthStencilState>>,
    cull: MTLCullMode,
) {
    e.setRenderPipelineState(state);
    if let Some(depth) = depth {
        e.setDepthStencilState(Some(depth));
    }
    e.setCullMode(cull);
}
