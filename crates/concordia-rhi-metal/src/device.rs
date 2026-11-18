//! `MetalDevice`: resources in generation-checked slots, pipelines, timelines and submission.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use concordia_rhi::{
    BindlessIndex, Buffer, BufferDesc, BufferUsage, ComputePipelineDesc, Device, DeviceCaps,
    GraphicsPipelineDesc, Heap, HeapDesc, MemoryLocation, MeshPipelineDesc, Pipeline, RhiError,
    SampleCount, ShaderSource, Slots, Submit, Texture, TextureDesc, TextureUsage, Timeline,
};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBuffer, MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandQueue, MTLComputePipelineState,
    MTLCullMode, MTLDepthStencilDescriptor, MTLDepthStencilState, MTLDevice, MTLEvent, MTLFunction,
    MTLHazardTrackingMode, MTLHeap, MTLHeapDescriptor, MTLHeapType, MTLLibrary,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLResource, MTLSharedEvent, MTLSize,
    MTLTextureDescriptor, MTLTextureType,
};

use crate::bindless::{MtlBuffer, MtlTexture, Table, buffer_entry, texture_entry};
use crate::convert;
use crate::probe::{self, Device as MtlDevice};

pub(crate) type MtlHeap = ProtocolObject<dyn MTLHeap>;
pub(crate) type MtlEvent = ProtocolObject<dyn MTLSharedEvent>;

#[derive(Debug)]
pub(crate) struct BufferEntry {
    pub(crate) raw: Retained<MtlBuffer>,
    pub(crate) desc: BufferDesc,
    pub(crate) bindless: Option<u32>,
}

#[derive(Debug)]
pub(crate) struct TextureEntry {
    pub(crate) raw: Retained<MtlTexture>,
    pub(crate) desc: TextureDesc,
    pub(crate) bindless: Option<u32>,
}

#[derive(Debug)]
pub(crate) enum PipelineEntry {
    Compute {
        state: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
        threads: MTLSize,
    },
    Graphics {
        state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
        depth: Option<Retained<ProtocolObject<dyn MTLDepthStencilState>>>,
        cull: MTLCullMode,
    },
}

/// Everything mutable, behind one lock.
#[derive(Debug)]
pub(crate) struct Inner {
    pub(crate) device: Retained<MtlDevice>,
    pub(crate) queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pub(crate) buffers: Slots<BufferEntry>,
    pub(crate) textures: Slots<TextureEntry>,
    pub(crate) pipelines: Slots<PipelineEntry>,
    pub(crate) heaps: Slots<Retained<MtlHeap>>,
    pub(crate) timelines: Slots<Retained<MtlEvent>>,
    pub(crate) buffer_table: Table,
    pub(crate) texture_table: Table,
    pub(crate) last: Option<Retained<ProtocolObject<dyn MTLCommandBuffer>>>,
}

// SAFETY: Metal devices, queues and resources are thread-safe objects; all access goes through the Mutex.
unsafe impl Send for Inner {}

/// A Metal device implementing the RHI.
#[derive(Debug)]
pub struct MetalDevice {
    caps: DeviceCaps,
    inner: Mutex<Inner>,
    lost: AtomicBool,
}

fn stale(kind: &'static str, index: u32, generation: u32) -> RhiError {
    RhiError::StaleHandle {
        kind,
        index,
        generation,
    }
}

impl MetalDevice {
    /// The system default GPU, headless.
    pub fn system_default() -> Result<Self, RhiError> {
        let device = probe::default_device()?;
        let caps = probe::caps(&device);
        let queue = device.newCommandQueue().ok_or_else(|| RhiError::NoDevice {
            reason: "could not create a command queue".into(),
        })?;
        let inner = Inner {
            buffer_table: Table::new(&device, "buffer")?,
            texture_table: Table::new(&device, "texture")?,
            device,
            queue,
            buffers: Slots::default(),
            textures: Slots::default(),
            pipelines: Slots::default(),
            heaps: Slots::default(),
            timelines: Slots::default(),
            last: None,
        };
        Ok(Self {
            caps,
            inner: Mutex::new(inner),
            lost: AtomicBool::new(false),
        })
    }

    /// Test hook: makes every later submission fail with `DeviceLost`.
    pub fn simulate_device_lost(&self) {
        self.lost.store(true, Ordering::SeqCst);
    }

    fn lock(&self) -> Result<MutexGuard<'_, Inner>, RhiError> {
        self.inner
            .lock()
            .map_err(|_| RhiError::DeviceLost("device state poisoned by a panic".into()))
    }

    fn texture_descriptor(desc: &TextureDesc) -> Result<Retained<MTLTextureDescriptor>, RhiError> {
        desc.validate()?;
        let d = MTLTextureDescriptor::new();
        d.setTextureType(match desc.samples {
            SampleCount::X1 => MTLTextureType::Type2D,
            _ => MTLTextureType::Type2DMultisample,
        });
        d.setPixelFormat(convert::pixel_format(desc.format)?);
        // SAFETY: sizes are validated (non-zero, ≤ 16384, mips within range, samples 1/2/4/8).
        unsafe {
            d.setWidth(desc.width.get() as usize);
            d.setHeight(desc.height.get() as usize);
            d.setMipmapLevelCount(desc.mip_levels.get() as usize);
            d.setSampleCount(usize::from(desc.samples.get()));
        }
        d.setUsage(convert::texture_usage(desc.usage));
        d.setStorageMode(convert::private_storage());
        d.setHazardTrackingMode(MTLHazardTrackingMode::Tracked);
        Ok(d)
    }

    fn register_texture(
        &self,
        inner: &mut Inner,
        raw: Retained<MtlTexture>,
        desc: &TextureDesc,
    ) -> Result<Texture, RhiError> {
        let bindless = match desc.usage.contains(TextureUsage::BINDLESS) {
            true => Some(inner.texture_table.insert(texture_entry(&raw))?),
            false => None,
        };
        let (index, generation) = inner.textures.insert(TextureEntry {
            raw,
            desc: desc.clone(),
            bindless,
        });
        Ok(Texture::from_raw(index, generation))
    }

    fn library(
        inner: &Inner,
        source: &ShaderSource,
        label: &str,
    ) -> Result<(Retained<ProtocolObject<dyn MTLLibrary>>, String), RhiError> {
        let (text, entry) = match source {
            ShaderSource::Msl { source, entry } => (source, entry),
            ShaderSource::SpirV { .. } => {
                return Err(RhiError::Unsupported(
                    "SPIR-V shaders on the Metal backend; compile Slang to MSL".into(),
                ));
            }
        };
        let library = inner
            .device
            .newLibraryWithSource_options_error(&NSString::from_str(text), None)
            .map_err(|e| RhiError::PipelineCompile {
                label: label.into(),
                log: e.localizedDescription().to_string(),
            })?;
        Ok((library, entry.clone()))
    }

    fn function(
        inner: &Inner,
        source: &ShaderSource,
        label: &str,
    ) -> Result<Retained<ProtocolObject<dyn MTLFunction>>, RhiError> {
        let (library, entry) = Self::library(inner, source, label)?;
        library
            .newFunctionWithName(&NSString::from_str(&entry))
            .ok_or_else(|| RhiError::PipelineCompile {
                label: label.into(),
                log: format!("no entry point {entry:?} in the compiled library"),
            })
    }

    fn depth_state(
        inner: &Inner,
        desc: &GraphicsPipelineDesc,
    ) -> Option<Retained<ProtocolObject<dyn MTLDepthStencilState>>> {
        let state = desc.depth?;
        let d = MTLDepthStencilDescriptor::new();
        d.setDepthCompareFunction(convert::compare(state.compare));
        d.setDepthWriteEnabled(state.write);
        inner.device.newDepthStencilStateWithDescriptor(&d)
    }
}

impl Device for MetalDevice {
    fn caps(&self) -> &DeviceCaps {
        &self.caps
    }

    fn create_buffer(&self, desc: &BufferDesc) -> Result<Buffer, RhiError> {
        desc.validate()?;
        let mut inner = self.lock()?;
        let size = usize::try_from(desc.size.get())
            .map_err(|_| RhiError::invalid("buffer desc", "size overflows usize"))?;
        let raw = inner
            .device
            .newBufferWithLength_options(size, convert::buffer_options(desc.memory))
            .ok_or_else(|| {
                RhiError::OutOfMemory(format!("buffer {:?} ({size} bytes)", desc.label))
            })?;
        raw.setLabel(Some(&NSString::from_str(&desc.label)));
        let bindless = match desc.usage.contains(BufferUsage::BINDLESS) {
            true => Some(inner.buffer_table.insert(buffer_entry(&raw))?),
            false => None,
        };
        let (index, generation) = inner.buffers.insert(BufferEntry {
            raw,
            desc: desc.clone(),
            bindless,
        });
        Ok(Buffer::from_raw(index, generation))
    }

    fn create_texture(&self, desc: &TextureDesc) -> Result<Texture, RhiError> {
        let d = Self::texture_descriptor(desc)?;
        let mut inner = self.lock()?;
        let raw = inner
            .device
            .newTextureWithDescriptor(&d)
            .ok_or_else(|| RhiError::OutOfMemory(format!("texture {:?}", desc.label)))?;
        raw.setLabel(Some(&NSString::from_str(&desc.label)));
        self.register_texture(&mut inner, raw, desc)
    }

    fn create_heap(&self, desc: &HeapDesc) -> Result<Heap, RhiError> {
        let mut inner = self.lock()?;
        let d = MTLHeapDescriptor::new();
        d.setSize(
            usize::try_from(desc.size.get())
                .map_err(|_| RhiError::invalid("heap desc", "size overflows usize"))?,
        );
        d.setStorageMode(convert::private_storage());
        d.setHazardTrackingMode(MTLHazardTrackingMode::Tracked);
        d.setType(MTLHeapType::Placement);
        let raw = inner
            .device
            .newHeapWithDescriptor(&d)
            .ok_or_else(|| RhiError::OutOfMemory(format!("heap {:?}", desc.label)))?;
        let (index, generation) = inner.heaps.insert(raw);
        Ok(Heap::from_raw(index, generation))
    }

    fn texture_heap_footprint(&self, desc: &TextureDesc) -> Result<(u64, u64), RhiError> {
        let d = Self::texture_descriptor(desc)?;
        let inner = self.lock()?;
        let sa = inner.device.heapTextureSizeAndAlignWithDescriptor(&d);
        Ok((sa.size as u64, sa.align as u64))
    }

    fn create_placed_texture(
        &self,
        heap: Heap,
        offset: u64,
        desc: &TextureDesc,
    ) -> Result<Texture, RhiError> {
        let d = Self::texture_descriptor(desc)?;
        let mut inner = self.lock()?;
        let (size, align) = {
            let sa = inner.device.heapTextureSizeAndAlignWithDescriptor(&d);
            (sa.size as u64, sa.align as u64)
        };
        let raw_heap = inner
            .heaps
            .get(heap.index(), heap.generation())
            .ok_or_else(|| stale("heap", heap.index(), heap.generation()))?;
        if !offset.is_multiple_of(align) || offset + size > raw_heap.size() as u64 {
            return Err(RhiError::invalid(
                "placed texture",
                format!(
                    "offset {offset} (align {align}) + size {size} must fit heap of {}",
                    raw_heap.size()
                ),
            ));
        }
        // SAFETY: offset is aligned and the texture fits inside the heap (checked above).
        let raw = unsafe { raw_heap.newTextureWithDescriptor_offset(&d, offset as usize) }
            .ok_or_else(|| RhiError::OutOfMemory(format!("placed texture {:?}", desc.label)))?;
        self.register_texture(&mut inner, raw, desc)
    }

    fn create_compute_pipeline(&self, desc: &ComputePipelineDesc) -> Result<Pipeline, RhiError> {
        let [x, y, z] = desc.threads_per_group;
        if x == 0 || y == 0 || z == 0 || x * y * z > 1024 {
            return Err(RhiError::invalid(
                "compute pipeline",
                format!(
                    "threads_per_group {:?} must be non-zero and ≤ 1024 in total",
                    desc.threads_per_group
                ),
            ));
        }
        let mut inner = self.lock()?;
        let function = Self::function(&inner, &desc.shader, &desc.label)?;
        let state = inner
            .device
            .newComputePipelineStateWithFunction_error(&function)
            .map_err(|e| RhiError::PipelineCompile {
                label: desc.label.clone(),
                log: e.localizedDescription().to_string(),
            })?;
        let threads = MTLSize {
            width: x as usize,
            height: y as usize,
            depth: z as usize,
        };
        let (index, generation) = inner
            .pipelines
            .insert(PipelineEntry::Compute { state, threads });
        Ok(Pipeline::from_raw(index, generation))
    }

    fn create_graphics_pipeline(&self, desc: &GraphicsPipelineDesc) -> Result<Pipeline, RhiError> {
        if desc.color_formats.is_empty() && desc.depth.is_none() || desc.color_formats.len() > 8 {
            return Err(RhiError::invalid(
                "graphics pipeline",
                "needs 1..=8 color targets or a depth target",
            ));
        }
        let mut inner = self.lock()?;
        let d = MTLRenderPipelineDescriptor::new();
        d.setLabel(Some(&NSString::from_str(&desc.label)));
        let vertex = Self::function(&inner, &desc.vertex, &desc.label)?;
        d.setVertexFunction(Some(&*vertex));
        if let Some(fragment) = &desc.fragment {
            let fragment = Self::function(&inner, fragment, &desc.label)?;
            d.setFragmentFunction(Some(&*fragment));
        }
        for (i, format) in desc.color_formats.iter().enumerate() {
            // SAFETY: i < 8, within Metal's color attachment array.
            let attachment = unsafe { d.colorAttachments().objectAtIndexedSubscript(i) };
            attachment.setPixelFormat(convert::pixel_format(*format)?);
            if let Some((src, dst)) = convert::blend(desc.blend) {
                attachment.setBlendingEnabled(true);
                attachment.setSourceRGBBlendFactor(src);
                attachment.setDestinationRGBBlendFactor(dst);
                attachment.setSourceAlphaBlendFactor(src);
                attachment.setDestinationAlphaBlendFactor(dst);
            }
        }
        if let Some(depth) = desc.depth {
            d.setDepthAttachmentPixelFormat(convert::pixel_format(depth.format)?);
        }
        d.setRasterSampleCount(usize::from(desc.samples.get()));
        let state = inner
            .device
            .newRenderPipelineStateWithDescriptor_error(&d)
            .map_err(|e| RhiError::PipelineCompile {
                label: desc.label.clone(),
                log: e.localizedDescription().to_string(),
            })?;
        let depth = Self::depth_state(&inner, desc);
        let entry = PipelineEntry::Graphics {
            state,
            depth,
            cull: convert::cull(desc.cull),
        };
        let (index, generation) = inner.pipelines.insert(entry);
        Ok(Pipeline::from_raw(index, generation))
    }

    fn create_mesh_pipeline(&self, desc: &MeshPipelineDesc) -> Result<Pipeline, RhiError> {
        Err(RhiError::Unsupported(format!(
            "mesh pipeline {:?}: mesh shaders arrive with virtualized geometry (M4)",
            desc.label
        )))
    }

    fn write_buffer(&self, buffer: Buffer, offset: u64, data: &[u8]) -> Result<(), RhiError> {
        let inner = self.lock()?;
        let entry = inner
            .buffers
            .get(buffer.index(), buffer.generation())
            .ok_or_else(|| stale("buffer", buffer.index(), buffer.generation()))?;
        check_cpu_range(entry, offset, data.len() as u64)?;
        let dst = entry.raw.contents().as_ptr().cast::<u8>();
        // SAFETY: the range is inside a CPU-visible (shared) buffer, checked above.
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst.add(offset as usize), data.len())
        };
        Ok(())
    }

    fn read_buffer(&self, buffer: Buffer, offset: u64, len: u64) -> Result<Vec<u8>, RhiError> {
        let inner = self.lock()?;
        let entry = inner
            .buffers
            .get(buffer.index(), buffer.generation())
            .ok_or_else(|| stale("buffer", buffer.index(), buffer.generation()))?;
        check_cpu_range(entry, offset, len)?;
        let src = entry.raw.contents().as_ptr().cast::<u8>();
        let mut out = vec![0; len as usize];
        // SAFETY: the range is inside a CPU-visible (shared) buffer, checked above.
        unsafe {
            std::ptr::copy_nonoverlapping(src.add(offset as usize), out.as_mut_ptr(), out.len())
        };
        Ok(out)
    }

    fn buffer_index(&self, buffer: Buffer) -> Result<BindlessIndex, RhiError> {
        let inner = self.lock()?;
        let entry = inner
            .buffers
            .get(buffer.index(), buffer.generation())
            .ok_or_else(|| stale("buffer", buffer.index(), buffer.generation()))?;
        entry.bindless.map(BindlessIndex).ok_or_else(|| {
            RhiError::invalid(
                "buffer_index",
                format!(
                    "{:?} was not created with BufferUsage::BINDLESS",
                    entry.desc.label
                ),
            )
        })
    }

    fn texture_index(&self, texture: Texture) -> Result<BindlessIndex, RhiError> {
        let inner = self.lock()?;
        let entry = inner
            .textures
            .get(texture.index(), texture.generation())
            .ok_or_else(|| stale("texture", texture.index(), texture.generation()))?;
        entry.bindless.map(BindlessIndex).ok_or_else(|| {
            RhiError::invalid(
                "texture_index",
                format!(
                    "{:?} was not created with TextureUsage::BINDLESS",
                    entry.desc.label
                ),
            )
        })
    }

    fn create_timeline(&self, initial: u64) -> Result<Timeline, RhiError> {
        let mut inner = self.lock()?;
        let event = inner
            .device
            .newSharedEvent()
            .ok_or_else(|| RhiError::OutOfMemory("shared event".into()))?;
        event.setSignaledValue(initial);
        let (index, generation) = inner.timelines.insert(event);
        Ok(Timeline::from_raw(index, generation))
    }

    fn timeline_value(&self, timeline: Timeline) -> Result<u64, RhiError> {
        let inner = self.lock()?;
        let event = inner
            .timelines
            .get(timeline.index(), timeline.generation())
            .ok_or_else(|| stale("timeline", timeline.index(), timeline.generation()))?;
        Ok(event.signaledValue())
    }

    fn wait_timeline(
        &self,
        timeline: Timeline,
        value: u64,
        timeout: Duration,
    ) -> Result<bool, RhiError> {
        let event = {
            let inner = self.lock()?;
            inner
                .timelines
                .get(timeline.index(), timeline.generation())
                .ok_or_else(|| stale("timeline", timeline.index(), timeline.generation()))?
                .clone()
        };
        let ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        Ok(event.waitUntilSignaledValue_timeoutMS(value, ms))
    }

    fn submit(&self, submit: Submit<'_>) -> Result<(), RhiError> {
        submit.commands.validate()?;
        if self.lost.load(Ordering::SeqCst) {
            return Err(RhiError::DeviceLost(
                "simulated device loss (test hook)".into(),
            ));
        }
        let mut inner = self.lock()?;
        let command_buffer = crate::encode::record(&inner, &submit)?;
        command_buffer.commit();
        inner.last = Some(command_buffer);
        Ok(())
    }

    fn wait_idle(&self) -> Result<(), RhiError> {
        let last = self.lock()?.last.clone();
        let Some(command_buffer) = last else {
            return Ok(());
        };
        command_buffer.waitUntilCompleted();
        match command_buffer.status() {
            MTLCommandBufferStatus::Error => {
                Err(RhiError::DeviceLost(command_buffer.error().map_or_else(
                    || "command buffer failed".into(),
                    |e| e.localizedDescription().to_string(),
                )))
            }
            _ => Ok(()),
        }
    }

    fn destroy_buffer(&self, buffer: Buffer) {
        let Ok(mut inner) = self.lock() else { return };
        if let Some(entry) = inner.buffers.remove(buffer.index(), buffer.generation())
            && let Some(slot) = entry.bindless
        {
            inner.buffer_table.release(slot);
        }
    }

    fn destroy_texture(&self, texture: Texture) {
        let Ok(mut inner) = self.lock() else { return };
        if let Some(entry) = inner.textures.remove(texture.index(), texture.generation())
            && let Some(slot) = entry.bindless
        {
            inner.texture_table.release(slot);
        }
    }
}

/// CPU access must stay inside a CPU-visible buffer.
fn check_cpu_range(entry: &BufferEntry, offset: u64, len: u64) -> Result<(), RhiError> {
    if entry.desc.memory == MemoryLocation::GpuOnly {
        return Err(RhiError::invalid(
            "buffer access",
            format!(
                "{:?} is GpuOnly; copy it to a Readback buffer",
                entry.desc.label
            ),
        ));
    }
    match offset.checked_add(len) {
        Some(end) if end <= entry.desc.size.get() => Ok(()),
        _ => Err(RhiError::invalid(
            "buffer access",
            format!(
                "{offset}+{len} exceeds {:?} ({} bytes)",
                entry.desc.label, entry.desc.size
            ),
        )),
    }
}

/// Keeps Metal event types in scope for `encode`.
pub(crate) fn as_event(event: &MtlEvent) -> &ProtocolObject<dyn MTLEvent> {
    ProtocolObject::from_ref(event)
}
