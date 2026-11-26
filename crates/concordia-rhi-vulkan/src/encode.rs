//! Translates a planned command list into a Vulkan command buffer.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use ash::vk;
use concordia_rhi::{
    Buffer, BufferUsage, Command, DepthResolve, DepthResolveTarget, Format, Layout, Pipeline, Rect,
    RenderPassDesc, RhiError, SampleCount, Texture, TextureOrigin, TextureUsage, TimerScope,
    Viewport, check,
};

use crate::convert;
use crate::device::Resources;
use crate::pipeline::Kind;
use crate::resources::{VkBuffer, VkTexture, range};
use crate::timing::Queries;
use crate::track::{self, Op, TexState, Transition};

/// Where a command buffer records into, and what it binds.
pub(crate) struct Target<'a> {
    pub device: &'a ash::Device,
    pub cb: vk::CommandBuffer,
    pub layout: vk::PipelineLayout,
    /// The layout's push-constant stages.
    pub stages: vk::ShaderStageFlags,
    pub sets: [vk::DescriptorSet; 3],
    /// `VK_EXT_mesh_shader`'s commands, when enabled.
    pub mesh: Option<&'a ash::ext::mesh_shader::Device>,
    /// Timestamp queries, when timed.
    pub queries: Option<(vk::QueryPool, &'a Queries)>,
    /// The device resolves depth with the min mode.
    pub depth_resolve_min: bool,
}

/// Records `commands` after `inits` (new textures' moves to their home layouts). Returns the
/// textures' states after the list, to commit once the submission is accepted.
pub(crate) fn record(
    target: &Target<'_>,
    resources: &Resources,
    commands: &[Command],
    inits: &[Texture],
) -> Result<BTreeMap<Texture, TexState>, RhiError> {
    let mut states = named_textures(resources, commands)?;
    let ops = track::plan(commands, &mut states)
        .map_err(|t| stale("texture", t.index(), t.generation()))?;
    let mut recorder = Recorder {
        t: target,
        resources,
        bound: None,
        in_pass: false,
        timer: None,
        pass_size: None,
    };
    recorder.begin(inits)?;
    for op in &ops {
        let step = match op {
            Op::Barrier(transitions) => recorder.barrier(transitions),
            Op::Command(n) => recorder.command(&commands[*n]),
        };
        if let Err(e) = step {
            recorder.abandon();
            return Err(e);
        }
    }
    recorder.finish();
    Ok(states)
}

pub(crate) fn stale(kind: &'static str, index: u32, generation: u32) -> RhiError {
    RhiError::StaleHandle {
        kind,
        index,
        generation,
    }
}

/// Every texture the list names, with its current state.
fn named_textures(
    resources: &Resources,
    commands: &[Command],
) -> Result<BTreeMap<Texture, TexState>, RhiError> {
    let mut named = BTreeMap::new();
    for texture in commands.iter().flat_map(textures_of) {
        named.insert(texture, resources.texture(texture)?.state);
    }
    Ok(named)
}

fn textures_of(command: &Command) -> Vec<Texture> {
    match command {
        Command::BeginRenderPass(desc) => {
            let mut v: Vec<Texture> = desc
                .colors
                .iter()
                .flat_map(|c| [Some(c.texture), c.resolve])
                .flatten()
                .collect();
            if let Some(d) = &desc.depth {
                v.push(d.texture);
                v.extend(d.resolve.map(|r| r.texture));
            }
            v
        }
        Command::CopyBufferToTexture { dst, .. } => vec![*dst],
        Command::CopyTextureToBuffer { src, .. } => vec![*src],
        Command::CopyTexture { src, dst, .. } => vec![*src, *dst],
        Command::Barrier { textures, .. } => textures.iter().map(|b| b.texture).collect(),
        _ => Vec::new(),
    }
}

struct Recorder<'a> {
    t: &'a Target<'a>,
    resources: &'a Resources,
    bound: Option<Kind>,
    in_pass: bool,
    timer: Option<TimerScope>,
    /// The open render pass's target size (width, height).
    pass_size: Option<[NonZeroU32; 2]>,
}

fn invalid(reason: impl Into<String>) -> RhiError {
    RhiError::invalid("command", reason)
}

/// A dependency on everything before by everything after, through all memory.
fn global() -> vk::MemoryBarrier2<'static> {
    vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
}

impl Recorder<'_> {
    fn device(&self) -> &ash::Device {
        self.t.device
    }

    fn begin(&mut self, inits: &[Texture]) -> Result<(), RhiError> {
        for point in [
            vk::PipelineBindPoint::GRAPHICS,
            vk::PipelineBindPoint::COMPUTE,
        ] {
            // SAFETY: the command buffer is recording; the sets match the layout.
            unsafe {
                self.device().cmd_bind_descriptor_sets(
                    self.t.cb,
                    point,
                    self.t.layout,
                    0,
                    &self.t.sets,
                    &[],
                );
            }
        }
        if let Some((pool, queries)) = self.t.queries {
            // SAFETY: recording outside a render pass; the pool holds `queries.count` queries.
            unsafe {
                self.device()
                    .cmd_reset_query_pool(self.t.cb, pool, 0, queries.count);
                self.device().cmd_write_timestamp2(
                    self.t.cb,
                    vk::PipelineStageFlags2::ALL_COMMANDS,
                    pool,
                    0,
                );
            }
        }
        let transitions: Vec<Transition> = inits
            .iter()
            .filter_map(|t| {
                self.resources
                    .texture(*t)
                    .ok()
                    .map(|tex| (*t, tex.state.home))
            })
            .map(|(texture, home)| Transition {
                texture,
                from: Layout::Undefined,
                to: home,
            })
            .collect();
        match transitions.is_empty() {
            true => Ok(()),
            false => self.barrier(&transitions),
        }
    }

    /// Leaves the command buffer endable after a failed command: closes an open render pass.
    fn abandon(&mut self) {
        if self.in_pass {
            // SAFETY: a render pass is open on this command buffer.
            unsafe { self.device().cmd_end_rendering(self.t.cb) };
            self.in_pass = false;
        }
    }

    fn finish(&mut self) {
        let to_host = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .dst_access_mask(vk::AccessFlags2::HOST_READ);
        let dep = [to_host];
        // SAFETY: recording outside a render pass (the list was validated closed).
        unsafe {
            self.device().cmd_pipeline_barrier2(
                self.t.cb,
                &vk::DependencyInfo::default().memory_barriers(&dep),
            );
        }
        if let Some((pool, queries)) = self.t.queries {
            // SAFETY: the last query is reserved for the list's end.
            unsafe {
                self.device().cmd_write_timestamp2(
                    self.t.cb,
                    vk::PipelineStageFlags2::ALL_COMMANDS,
                    pool,
                    queries.count - 1,
                );
            }
        }
    }

    fn barrier(&mut self, transitions: &[Transition]) -> Result<(), RhiError> {
        let mut images = Vec::with_capacity(transitions.len());
        for tr in transitions {
            let texture = self.resources.texture(tr.texture)?;
            images.push(
                vk::ImageMemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
                    .old_layout(convert::layout(tr.from))
                    .new_layout(convert::layout(tr.to))
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(texture.raw)
                    .subresource_range(range(&texture.desc, texture.desc.mip_levels.get())),
            );
        }
        let memory = [global()];
        let dep = vk::DependencyInfo::default()
            .memory_barriers(&memory)
            .image_memory_barriers(&images);
        // SAFETY: planner barriers are never inside a render pass; the images are live.
        unsafe { self.device().cmd_pipeline_barrier2(self.t.cb, &dep) };
        Ok(())
    }

    fn command(&mut self, command: &Command) -> Result<(), RhiError> {
        match command {
            Command::Barrier { .. } => Ok(()),
            Command::CopyBuffer {
                src,
                src_offset,
                dst,
                dst_offset,
                size,
            } => self.copy_buffer(*src, *src_offset, *dst, *dst_offset, *size),
            Command::CopyBufferToTexture {
                src,
                offset,
                bytes_per_row,
                dst,
                mip,
            } => self.copy_image(*src, *offset, *bytes_per_row, *dst, *mip, true),
            Command::CopyTextureToBuffer {
                src,
                mip,
                dst,
                offset,
                bytes_per_row,
            } => self.copy_image(*dst, *offset, *bytes_per_row, *src, *mip, false),
            Command::BeginRenderPass(desc) => self.begin_pass(desc),
            Command::EndRenderPass => {
                self.in_pass = false;
                self.pass_size = None;
                // SAFETY: a render pass is open (validated list).
                unsafe { self.device().cmd_end_rendering(self.t.cb) };
                Ok(())
            }
            Command::SetPipeline(p) => self.set_pipeline(*p),
            Command::PushConstants(bytes) => {
                self.push(bytes);
                Ok(())
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
            } => self.draw_indexed(*indices, *offset, *count, instances.clone()),
            Command::DrawMeshTasks { groups } => self.draw_mesh_tasks(*groups),
            Command::Dispatch { groups } => {
                self.require(Kind::Compute)?;
                // SAFETY: a compute pipeline is bound outside a render pass.
                unsafe {
                    self.device()
                        .cmd_dispatch(self.t.cb, groups[0], groups[1], groups[2])
                };
                Ok(())
            }
            Command::DispatchIndirect { buffer, offset } => {
                self.dispatch_indirect(*buffer, *offset)
            }
            Command::BeginTimer(scope) => {
                self.timer = Some(*scope);
                self.timestamp(self.t.queries.and_then(|(_, q)| q.begin(*scope)));
                Ok(())
            }
            Command::EndTimer => {
                let scope = self.timer.take();
                self.timestamp(self.t.queries.zip(scope).and_then(|((_, q), s)| q.end(s)));
                Ok(())
            }
            Command::DrawIndirect { args, offset } => self.draw_indirect(None, *args, *offset),
            Command::DrawIndexedIndirect {
                indices,
                args,
                offset,
            } => self.draw_indirect(Some(*indices), *args, *offset),
            Command::SetViewport(viewport) => self.set_viewport(viewport),
            Command::SetScissor(rect) => self.set_scissor(rect),
            Command::CopyTexture {
                src,
                src_origin,
                dst,
                dst_origin,
                extent,
            } => self.copy_texture((*src, *src_origin), (*dst, *dst_origin), *extent),
        }
    }

    fn timestamp(&self, query: Option<u32>) {
        if let (Some((pool, _)), Some(query)) = (self.t.queries, query) {
            // SAFETY: timers sit outside render passes; the query was reset at the list's start.
            unsafe {
                self.device().cmd_write_timestamp2(
                    self.t.cb,
                    vk::PipelineStageFlags2::ALL_COMMANDS,
                    pool,
                    query,
                );
            }
        }
    }

    fn buffer(
        &self,
        handle: Buffer,
        usage: BufferUsage,
        needs: &str,
    ) -> Result<&VkBuffer, RhiError> {
        let buffer = self.resources.buffer(handle)?;
        match buffer.desc.usage.contains(usage) {
            true => Ok(buffer),
            false => Err(invalid(format!(
                "{:?} lacks {usage:?} usage for {needs}",
                buffer.desc.label
            ))),
        }
    }

    fn texture(
        &self,
        handle: Texture,
        usage: TextureUsage,
        needs: &str,
    ) -> Result<&VkTexture, RhiError> {
        let texture = self.resources.texture(handle)?;
        match texture.desc.usage.contains(usage) {
            true => Ok(texture),
            false => Err(invalid(format!(
                "{:?} lacks {usage:?} usage for {needs}",
                texture.desc.label
            ))),
        }
    }

    fn copy_buffer(
        &self,
        src: Buffer,
        src_offset: u64,
        dst: Buffer,
        dst_offset: u64,
        size: u64,
    ) -> Result<(), RhiError> {
        let s = self.buffer(src, BufferUsage::COPY_SRC, "a copy source")?;
        let d = self.buffer(dst, BufferUsage::COPY_DST, "a copy destination")?;
        within(s, src_offset, size)?;
        within(d, dst_offset, size)?;
        if size == 0 {
            return Ok(());
        }
        let region = [vk::BufferCopy {
            src_offset,
            dst_offset,
            size,
        }];
        // SAFETY: both ranges lie inside live buffers with copy usage; outside a render pass.
        unsafe {
            self.device()
                .cmd_copy_buffer(self.t.cb, s.raw, d.raw, &region)
        };
        Ok(())
    }

    /// Buffer ↔ texture copy (`upload`: buffer to texture).
    fn copy_image(
        &self,
        buffer: Buffer,
        offset: u64,
        bytes_per_row: u32,
        texture: Texture,
        mip: u32,
        upload: bool,
    ) -> Result<(), RhiError> {
        let (b_usage, t_usage) = match upload {
            true => (BufferUsage::COPY_SRC, TextureUsage::COPY_DST),
            false => (BufferUsage::COPY_DST, TextureUsage::COPY_SRC),
        };
        let b = self.buffer(buffer, b_usage, "a texture copy")?;
        let t = self.texture(texture, t_usage, "a texture copy")?;
        let region = image_region(&t.desc, b, offset, bytes_per_row, mip)?;
        let layout = convert::layout(track::copy_layout(
            t.state.home,
            if upload {
                Layout::CopyDst
            } else {
                Layout::CopySrc
            },
        ));
        // SAFETY: the region was checked against both resources; the planner moved the texture
        // into `layout`; outside a render pass.
        unsafe {
            match upload {
                true => self.device().cmd_copy_buffer_to_image(
                    self.t.cb,
                    b.raw,
                    t.raw,
                    layout,
                    &[region],
                ),
                false => self.device().cmd_copy_image_to_buffer(
                    self.t.cb,
                    t.raw,
                    layout,
                    b.raw,
                    &[region],
                ),
            }
        }
        Ok(())
    }

    fn begin_pass(&mut self, desc: &RenderPassDesc) -> Result<(), RhiError> {
        let mut extent = vk::Extent2D {
            width: u32::MAX,
            height: u32::MAX,
        };
        let mut colors = Vec::with_capacity(desc.colors.len());
        for color in &desc.colors {
            let t = self.texture(
                color.texture,
                TextureUsage::RENDER_TARGET,
                "a color attachment",
            )?;
            if t.desc.format.is_depth() {
                return Err(invalid(format!(
                    "{:?} is a depth texture in a color slot",
                    t.desc.label
                )));
            }
            shrink(&mut extent, t);
            let (load, clear) = convert::load(color.load);
            let mut info = vk::RenderingAttachmentInfo::default()
                .image_view(t.mip0)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(load)
                .store_op(convert::store(color.store))
                .clear_value(clear);
            if let Some(resolve) = color.resolve {
                let r = self.resolve_target(t, resolve)?;
                info = info
                    .resolve_mode(resolve_mode(t.desc.format))
                    .resolve_image_view(r.mip0)
                    .resolve_image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
            }
            colors.push(info);
        }
        let depth = match &desc.depth {
            Some(d) => {
                let t =
                    self.texture(d.texture, TextureUsage::RENDER_TARGET, "a depth attachment")?;
                if !t.desc.format.is_depth() {
                    return Err(invalid(format!(
                        "{:?} is not a depth texture",
                        t.desc.label
                    )));
                }
                shrink(&mut extent, t);
                let (load, clear) = convert::load(d.load);
                let info = vk::RenderingAttachmentInfo::default()
                    .image_view(t.mip0)
                    .image_layout(convert::layout(Layout::DepthAttachment))
                    .load_op(load)
                    .store_op(convert::store(d.store))
                    .clear_value(clear);
                Some(match d.resolve {
                    Some(resolve) => self.depth_resolve(info, t, resolve)?,
                    None => info,
                })
            }
            None => None,
        };
        let area = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        };
        let mut info = vk::RenderingInfo::default()
            .render_area(area)
            .layer_count(1)
            .color_attachments(&colors);
        if let Some(depth) = depth.as_ref() {
            info = info.depth_attachment(depth);
        }
        // Y up, as on Metal: a negative-height viewport (ADR-0088).
        let viewport = vk::Viewport {
            x: 0.0,
            y: extent.height as f32,
            width: extent.width as f32,
            height: -(extent.height as f32),
            min_depth: 0.0,
            max_depth: 1.0,
        };
        // SAFETY: the attachments are live, in attachment layouts (planner), outside any pass.
        unsafe {
            self.device().cmd_begin_rendering(self.t.cb, &info);
            self.device().cmd_set_viewport(self.t.cb, 0, &[viewport]);
            self.device().cmd_set_scissor(self.t.cb, 0, &[area]);
        }
        self.in_pass = true;
        self.pass_size = NonZeroU32::new(extent.width)
            .zip(NonZeroU32::new(extent.height))
            .map(|(w, h)| [w, h]);
        Ok(())
    }

    /// `info` resolving depth texture `source` into `resolve` (the shared check first).
    fn depth_resolve<'i>(
        &self,
        info: vk::RenderingAttachmentInfo<'i>,
        source: &VkTexture,
        resolve: DepthResolveTarget,
    ) -> Result<vk::RenderingAttachmentInfo<'i>, RhiError> {
        let r = self.texture(
            resolve.texture,
            TextureUsage::RENDER_TARGET,
            "a depth resolve target",
        )?;
        check::depth_resolve(&source.desc, &r.desc, self.t.depth_resolve_min)?;
        let mode = match resolve.filter {
            DepthResolve::Min => vk::ResolveModeFlags::MIN,
        };
        Ok(info
            .resolve_mode(mode)
            .resolve_image_view(r.mip0)
            .resolve_image_layout(convert::layout(Layout::DepthAttachment)))
    }

    fn resolve_target(&self, source: &VkTexture, resolve: Texture) -> Result<&VkTexture, RhiError> {
        let r = self.texture(resolve, TextureUsage::RENDER_TARGET, "a resolve target")?;
        if source.desc.samples == SampleCount::X1 || r.desc.samples != SampleCount::X1 {
            return Err(invalid(format!(
                "resolving {:?} into {:?} needs a multisampled source and a single-sample target",
                source.desc.label, r.desc.label
            )));
        }
        if (r.desc.width, r.desc.height, r.desc.format)
            != (source.desc.width, source.desc.height, source.desc.format)
        {
            return Err(invalid(format!(
                "{:?} and its resolve target differ in size or format",
                source.desc.label
            )));
        }
        Ok(r)
    }

    fn set_pipeline(&mut self, handle: Pipeline) -> Result<(), RhiError> {
        let pipeline = self.resources.pipeline(handle)?;
        let point = match (pipeline.kind, self.in_pass) {
            (Kind::Graphics | Kind::Mesh, true) => vk::PipelineBindPoint::GRAPHICS,
            (Kind::Compute, false) => vk::PipelineBindPoint::COMPUTE,
            (Kind::Graphics | Kind::Mesh, false) => {
                return Err(invalid("graphics pipelines are set inside render passes"));
            }
            (Kind::Compute, true) => {
                return Err(invalid("compute pipelines are set outside render passes"));
            }
        };
        // SAFETY: the pipeline is live and matches the bind point.
        unsafe {
            self.device()
                .cmd_bind_pipeline(self.t.cb, point, pipeline.raw)
        };
        self.bound = Some(pipeline.kind);
        Ok(())
    }

    fn push(&self, bytes: &[u8]) {
        let mut padded = bytes.to_vec();
        padded.resize(bytes.len().next_multiple_of(4), 0);
        if padded.is_empty() {
            return;
        }
        // SAFETY: at most 128 bytes (validated list), a multiple of 4, within the layout's range.
        unsafe {
            self.device()
                .cmd_push_constants(self.t.cb, self.t.layout, self.t.stages, 0, &padded)
        };
    }

    fn require(&self, kind: Kind) -> Result<(), RhiError> {
        match self.bound == Some(kind) {
            true => Ok(()),
            false => Err(invalid(format!("no {kind:?} pipeline is set"))),
        }
    }

    /// `groups` task (or mesh) groups of the bound mesh pipeline.
    fn draw_mesh_tasks(&self, groups: [u32; 3]) -> Result<(), RhiError> {
        self.require(Kind::Mesh)?;
        let mesh = self
            .t
            .mesh
            .ok_or_else(|| RhiError::Unsupported("mesh-task draws without mesh shaders".into()))?;
        let [x, y, z] = groups;
        // SAFETY: a mesh pipeline is bound inside a render pass; the counts are within the
        // portable limits (validated list) the device meets (`MeshRecord::meets_portable_limits`).
        unsafe { mesh.cmd_draw_mesh_tasks(self.t.cb, x, y, z) };
        Ok(())
    }

    fn draw(
        &self,
        vertices: std::ops::Range<u32>,
        instances: std::ops::Range<u32>,
    ) -> Result<(), RhiError> {
        self.require(Kind::Graphics)?;
        let (v, i) = (span(&vertices)?, span(&instances)?);
        // SAFETY: a graphics pipeline is bound inside a render pass.
        unsafe {
            self.device()
                .cmd_draw(self.t.cb, v, i, vertices.start, instances.start)
        };
        Ok(())
    }

    fn draw_indexed(
        &self,
        indices: Buffer,
        offset: u64,
        count: u32,
        instances: std::ops::Range<u32>,
    ) -> Result<(), RhiError> {
        self.require(Kind::Graphics)?;
        let b = self.buffer(indices, BufferUsage::INDEX, "indices")?;
        if !offset.is_multiple_of(4) {
            return Err(invalid(format!(
                "index offset {offset} is not 4-byte aligned"
            )));
        }
        within(b, offset, u64::from(count) * 4)?;
        let i = span(&instances)?;
        // SAFETY: the index range lies in a live index buffer; a graphics pipeline is bound.
        unsafe {
            self.device()
                .cmd_bind_index_buffer(self.t.cb, b.raw, offset, vk::IndexType::UINT32);
            self.device()
                .cmd_draw_indexed(self.t.cb, count, i, 0, 0, instances.start);
        }
        Ok(())
    }

    /// One indirect draw, indexed when `indices` is given, after the shared argument check.
    fn draw_indirect(
        &self,
        indices: Option<Buffer>,
        args: Buffer,
        offset: u64,
    ) -> Result<(), RhiError> {
        self.require(Kind::Graphics)?;
        let a = self.resources.buffer(args)?;
        let bytes = match indices {
            Some(_) => concordia_rhi::DRAW_INDEXED_INDIRECT_BYTES,
            None => concordia_rhi::DRAW_INDIRECT_BYTES,
        };
        check::indirect_args(&a.desc, offset, bytes)?;
        let stride = bytes as u32;
        match indices {
            Some(indices) => {
                let i = self.buffer(indices, BufferUsage::INDEX, "indices")?;
                // SAFETY: one argument record lies in a live indirect buffer (checked); the index
                // buffer is live; a graphics pipeline is bound inside a render pass.
                unsafe {
                    self.device()
                        .cmd_bind_index_buffer(self.t.cb, i.raw, 0, vk::IndexType::UINT32);
                    self.device()
                        .cmd_draw_indexed_indirect(self.t.cb, a.raw, offset, 1, stride);
                }
            }
            // SAFETY: as above, without indices (the pipeline pulls its vertices).
            None => unsafe {
                self.device()
                    .cmd_draw_indirect(self.t.cb, a.raw, offset, 1, stride)
            },
        }
        Ok(())
    }

    fn pass_size(&self) -> Result<[NonZeroU32; 2], RhiError> {
        self.pass_size
            .ok_or_else(|| invalid("no render pass is open"))
    }

    /// Maps clip space into `viewport`, flipped so Y stays up as on Metal (ADR-0088).
    fn set_viewport(&self, viewport: &Viewport) -> Result<(), RhiError> {
        check::viewport_in(viewport, self.pass_size()?)?;
        let flipped = flipped_viewport(viewport);
        // SAFETY: inside a render pass; viewports are dynamic state in every pipeline.
        unsafe { self.device().cmd_set_viewport(self.t.cb, 0, &[flipped]) };
        Ok(())
    }

    fn set_scissor(&self, rect: &Rect) -> Result<(), RhiError> {
        check::scissor_in(rect, self.pass_size()?)?;
        let area = vk::Rect2D {
            offset: vk::Offset2D {
                x: rect.x as i32,
                y: rect.y as i32,
            },
            extent: vk::Extent2D {
                width: rect.width.get(),
                height: rect.height.get(),
            },
        };
        // SAFETY: inside a render pass; the rectangle lies inside the targets (checked), so its
        // offsets fit an i32; scissors are dynamic state in every pipeline.
        unsafe { self.device().cmd_set_scissor(self.t.cb, 0, &[area]) };
        Ok(())
    }

    /// Copies one region between textures in the layouts the planner chose.
    fn copy_texture(
        &self,
        (src, from): (Texture, TextureOrigin),
        (dst, to): (Texture, TextureOrigin),
        extent: [NonZeroU32; 2],
    ) -> Result<(), RhiError> {
        let (s, d) = (self.resources.texture(src)?, self.resources.texture(dst)?);
        let side = |texture, desc, origin| check::CopySide {
            texture,
            desc,
            origin,
        };
        check::texture_copy(side(src, &s.desc, from), side(dst, &d.desc, to), extent)?;
        let (src_layout, dst_layout) = match src == dst {
            true => (Layout::General, Layout::General),
            false => (
                track::copy_layout(s.state.home, Layout::CopySrc),
                track::copy_layout(d.state.home, Layout::CopyDst),
            ),
        };
        let layers = |desc: &concordia_rhi::TextureDesc, mip| vk::ImageSubresourceLayers {
            aspect_mask: convert::aspect(desc.format),
            mip_level: mip,
            base_array_layer: 0,
            layer_count: 1,
        };
        let offset = |o: TextureOrigin| vk::Offset3D {
            x: o.x as i32,
            y: o.y as i32,
            z: 0,
        };
        let region = [vk::ImageCopy {
            src_subresource: layers(&s.desc, from.mip),
            src_offset: offset(from),
            dst_subresource: layers(&d.desc, to.mip),
            dst_offset: offset(to),
            extent: vk::Extent3D {
                width: extent[0].get(),
                height: extent[1].get(),
                depth: 1,
            },
        }];
        // SAFETY: both regions lie inside live single-sample textures with copy usage and don't
        // overlap (checked); the planner moved them into these layouts; outside a render pass.
        unsafe {
            self.device().cmd_copy_image(
                self.t.cb,
                s.raw,
                convert::layout(src_layout),
                d.raw,
                convert::layout(dst_layout),
                &region,
            )
        };
        Ok(())
    }

    fn dispatch_indirect(&self, buffer: Buffer, offset: u64) -> Result<(), RhiError> {
        self.require(Kind::Compute)?;
        let b = self.buffer(buffer, BufferUsage::INDIRECT, "dispatch arguments")?;
        if !offset.is_multiple_of(4) {
            return Err(invalid(format!(
                "indirect offset {offset} is not 4-byte aligned"
            )));
        }
        within(b, offset, 12)?;
        // SAFETY: three u32 arguments lie in a live indirect buffer; a compute pipeline is bound.
        unsafe {
            self.device()
                .cmd_dispatch_indirect(self.t.cb, b.raw, offset)
        };
        Ok(())
    }
}

fn span(range: &std::ops::Range<u32>) -> Result<u32, RhiError> {
    range
        .end
        .checked_sub(range.start)
        .ok_or_else(|| invalid(format!("range {range:?} ends before it starts")))
}

/// A Y-up viewport: Vulkan's framebuffer y points down, so the height is negated (core since 1.1).
fn flipped_viewport(v: &Viewport) -> vk::Viewport {
    vk::Viewport {
        x: v.x,
        y: v.y + v.height,
        width: v.width,
        height: -v.height,
        min_depth: v.min_depth,
        max_depth: v.max_depth,
    }
}

fn shrink(extent: &mut vk::Extent2D, t: &VkTexture) {
    extent.width = extent.width.min(t.desc.width.get());
    extent.height = extent.height.min(t.desc.height.get());
}

fn resolve_mode(format: Format) -> vk::ResolveModeFlags {
    match format {
        Format::R32Uint | Format::R64Uint => vk::ResolveModeFlags::SAMPLE_ZERO,
        _ => vk::ResolveModeFlags::AVERAGE,
    }
}

fn within(buffer: &VkBuffer, offset: u64, size: u64) -> Result<(), RhiError> {
    match offset.checked_add(size) {
        Some(end) if end <= buffer.desc.size.get() => Ok(()),
        _ => Err(invalid(format!(
            "{size} bytes at {offset} overflow {:?} ({} bytes)",
            buffer.desc.label, buffer.desc.size
        ))),
    }
}

/// The copy region for mip `mip` of `desc` with rows of `bytes_per_row` at `offset` in `buffer`.
fn image_region(
    desc: &concordia_rhi::TextureDesc,
    buffer: &VkBuffer,
    offset: u64,
    bytes_per_row: u32,
    mip: u32,
) -> Result<vk::BufferImageCopy, RhiError> {
    if mip >= desc.mip_levels.get() {
        return Err(invalid(format!("{:?} has no mip {mip}", desc.label)));
    }
    if desc.samples != SampleCount::X1 {
        return Err(invalid(format!(
            "{:?} is multisampled; resolve it before copying",
            desc.label
        )));
    }
    let bpp = desc.format.bytes_per_pixel();
    let (w, h) = (
        (desc.width.get() >> mip).max(1),
        (desc.height.get() >> mip).max(1),
    );
    if !bytes_per_row.is_multiple_of(bpp) || bytes_per_row < w * bpp {
        return Err(invalid(format!(
            "{bytes_per_row} bytes per row for {w} texels of {bpp} bytes ({:?})",
            desc.label
        )));
    }
    if !offset.is_multiple_of(u64::from(bpp.max(4))) {
        return Err(invalid(format!(
            "copy offset {offset} is not aligned to {} bytes",
            bpp.max(4)
        )));
    }
    within(
        buffer,
        offset,
        u64::from(bytes_per_row) * u64::from(h - 1) + u64::from(w * bpp),
    )?;
    Ok(vk::BufferImageCopy {
        buffer_offset: offset,
        buffer_row_length: bytes_per_row / bpp,
        buffer_image_height: 0,
        image_subresource: vk::ImageSubresourceLayers {
            aspect_mask: convert::aspect(desc.format),
            mip_level: mip,
            base_array_layer: 0,
            layer_count: 1,
        },
        image_offset: vk::Offset3D::default(),
        image_extent: vk::Extent3D {
            width: w,
            height: h,
            depth: 1,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flipped_viewports_keep_y_up_inside_their_rectangle() {
        let v = Viewport {
            x: 16.0,
            y: 8.0,
            width: 64.0,
            height: 32.0,
            min_depth: 0.25,
            max_depth: 0.75,
        };
        let f = flipped_viewport(&v);
        assert_eq!((f.x, f.width), (16.0, 64.0));
        // Clip y = +1 maps to y + height/2 · (1 − 1) = the rectangle's top edge (8), y = −1 to 40.
        let to_pixel = |clip_y: f32| f.y + f.height * (clip_y + 1.0) / 2.0;
        assert_eq!(to_pixel(1.0), 8.0, "clip top is the rectangle's top");
        assert_eq!(
            to_pixel(-1.0),
            40.0,
            "clip bottom is the rectangle's bottom"
        );
        assert_eq!((f.min_depth, f.max_depth), (0.25, 0.75));
    }
}
