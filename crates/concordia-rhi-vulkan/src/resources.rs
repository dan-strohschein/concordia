//! Buffers, textures and heaps: Vulkan objects with gpu-allocator memory.

use ash::vk;
use concordia_rhi::{BufferDesc, MemoryLocation, RhiError, TextureDesc, TextureUsage};
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};

use crate::convert::{self, error};
use crate::track::{self, TexState};

/// A live buffer.
pub(crate) struct VkBuffer {
    pub raw: vk::Buffer,
    pub allocation: Allocation,
    pub desc: BufferDesc,
    /// Its slot in the buffer address table.
    pub bindless: Option<u32>,
    /// Its device address (0 without one).
    pub address: vk::DeviceAddress,
}

/// A live texture.
pub(crate) struct VkTexture {
    pub raw: vk::Image,
    /// Every mip (sampling).
    pub view: vk::ImageView,
    /// Mip 0 only (attachments, storage); equal to `view` for single-mip textures.
    pub mip0: vk::ImageView,
    /// Its own memory, or `None` when placed in a heap.
    pub allocation: Option<Allocation>,
    pub desc: TextureDesc,
    pub state: TexState,
    /// Its slot in the texture tables.
    pub bindless: Option<u32>,
    /// A swapchain image: the swapchain owns the image and its view, so freeing skips them.
    pub swapchain: bool,
}

/// Memory for placed textures.
pub(crate) struct VkHeap {
    pub allocation: Allocation,
    pub size: u64,
    /// Memory types every placed texture must accept.
    pub memory_type_bits: u32,
}

/// Anything waiting for the GPU to finish before it is freed.
pub(crate) enum Dead {
    Buffer(VkBuffer),
    Texture(VkTexture),
    Heap(VkHeap),
    Pipeline(vk::Pipeline),
    Semaphore(vk::Semaphore),
}

/// What resource creation needs from the device.
pub(crate) struct Ctx<'a> {
    pub device: &'a ash::Device,
    pub allocator: &'a mut Allocator,
}

fn allocate(
    ctx: &mut Ctx<'_>,
    name: &str,
    requirements: vk::MemoryRequirements,
    location: gpu_allocator::MemoryLocation,
    linear: bool,
) -> Result<Allocation, RhiError> {
    ctx.allocator
        .allocate(&AllocationCreateDesc {
            name,
            requirements,
            location,
            linear,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        })
        .map_err(|e| RhiError::OutOfMemory(format!("{name:?}: {e}")))
}

/// Creates a buffer and binds its memory.
pub(crate) fn create_buffer(ctx: &mut Ctx<'_>, desc: &BufferDesc) -> Result<VkBuffer, RhiError> {
    let usage = convert::buffer_usage(desc.usage);
    let info = vk::BufferCreateInfo::default()
        .size(desc.size.get())
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    // SAFETY: `info` is valid; the buffer is destroyed on every failure path below.
    let raw =
        unsafe { ctx.device.create_buffer(&info, None) }.map_err(|e| error("vkCreateBuffer", e))?;
    // SAFETY: `raw` is a live buffer of this device.
    let requirements = unsafe { ctx.device.get_buffer_memory_requirements(raw) };
    let allocation = match allocate(
        ctx,
        &desc.label,
        requirements,
        convert::memory(desc.memory),
        true,
    ) {
        Ok(a) => a,
        Err(e) => {
            // SAFETY: `raw` is unused.
            unsafe { ctx.device.destroy_buffer(raw, None) };
            return Err(e);
        }
    };
    // SAFETY: the allocation satisfies `requirements` and is bound once.
    let bound = unsafe {
        ctx.device
            .bind_buffer_memory(raw, allocation.memory(), allocation.offset())
    };
    if let Err(e) = bound {
        free_buffer_parts(ctx, raw, allocation);
        return Err(error("vkBindBufferMemory", e));
    }
    let address = match usage.contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS) {
        // SAFETY: the buffer has device-address usage and bound memory.
        true => unsafe {
            ctx.device
                .get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(raw))
        },
        false => 0,
    };
    Ok(VkBuffer {
        raw,
        allocation,
        desc: desc.clone(),
        bindless: None,
        address,
    })
}

fn free_buffer_parts(ctx: &mut Ctx<'_>, raw: vk::Buffer, allocation: Allocation) {
    // SAFETY: the buffer is not in use by the GPU (callers retire it first).
    unsafe { ctx.device.destroy_buffer(raw, None) };
    let _ = ctx.allocator.free(allocation);
}

impl VkBuffer {
    /// Its mapped bytes, for `Upload` and `Readback` buffers.
    pub fn mapped(&mut self) -> Result<&mut [u8], RhiError> {
        if self.desc.memory == MemoryLocation::GpuOnly {
            return Err(RhiError::invalid(
                "buffer access",
                format!(
                    "{:?} is GpuOnly; only Upload and Readback buffers are CPU-visible",
                    self.desc.label
                ),
            ));
        }
        let size = self.desc.size.get() as usize;
        let label = self.desc.label.clone();
        self.allocation
            .mapped_slice_mut()
            .map(|s| {
                let len = size.min(s.len());
                &mut s[..len]
            })
            .ok_or_else(|| RhiError::invalid("buffer access", format!("{label:?} is not mapped")))
    }
}

/// The image create info for `desc`.
pub(crate) fn image_info(desc: &TextureDesc) -> vk::ImageCreateInfo<'static> {
    vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(convert::format(desc.format))
        .extent(vk::Extent3D {
            width: desc.width.get(),
            height: desc.height.get(),
            depth: 1,
        })
        .mip_levels(desc.mip_levels.get())
        .array_layers(1)
        .samples(convert::samples(desc.samples))
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(convert::texture_usage(desc.usage, desc.format))
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
}

/// Size, alignment and memory types a texture needs, without creating it.
pub(crate) fn requirements(device: &ash::Device, desc: &TextureDesc) -> vk::MemoryRequirements {
    let info = image_info(desc);
    let query = vk::DeviceImageMemoryRequirements::default().create_info(&info);
    let mut out = vk::MemoryRequirements2::default();
    // SAFETY: `info` is a valid image description (validated by the caller); `out` is writable.
    unsafe { device.get_device_image_memory_requirements(&query, &mut out) };
    out.memory_requirements
}

/// Where a texture's memory comes from.
pub(crate) enum Placement<'h> {
    /// Its own allocation.
    Own,
    /// `offset` bytes into a heap.
    Placed(&'h VkHeap, u64),
}

/// Creates a texture (image, views and memory) resting in its home layout once initialised.
pub(crate) fn create_texture(
    ctx: &mut Ctx<'_>,
    desc: &TextureDesc,
    placement: Placement<'_>,
) -> Result<VkTexture, RhiError> {
    // SAFETY: the description was validated (`TextureDesc::validate`, format support).
    let raw = unsafe { ctx.device.create_image(&image_info(desc), None) }
        .map_err(|e| error("vkCreateImage", e))?;
    let allocation = match bind_image(ctx, raw, desc, placement) {
        Ok(a) => a,
        Err(e) => {
            // SAFETY: `raw` is unused.
            unsafe { ctx.device.destroy_image(raw, None) };
            return Err(e);
        }
    };
    let views = create_views(ctx.device, raw, desc);
    let (view, mip0) = match views {
        Ok(v) => v,
        Err(e) => {
            // SAFETY: `raw` is unused.
            unsafe { ctx.device.destroy_image(raw, None) };
            if let Some(a) = allocation {
                let _ = ctx.allocator.free(a);
            }
            return Err(e);
        }
    };
    let state = TexState {
        home: track::home(desc.usage, desc.format),
        defined: true,
    };
    Ok(VkTexture {
        raw,
        view,
        mip0,
        allocation,
        desc: desc.clone(),
        state,
        bindless: None,
        swapchain: false,
    })
}

fn bind_image(
    ctx: &mut Ctx<'_>,
    raw: vk::Image,
    desc: &TextureDesc,
    placement: Placement<'_>,
) -> Result<Option<Allocation>, RhiError> {
    // SAFETY: `raw` is a live image of this device.
    let req = unsafe { ctx.device.get_image_memory_requirements(raw) };
    let (memory, offset, own) = match placement {
        Placement::Own => {
            let a = allocate(
                ctx,
                &desc.label,
                req,
                gpu_allocator::MemoryLocation::GpuOnly,
                false,
            )?;
            // SAFETY: reading the allocation's memory handle to bind it.
            (unsafe { a.memory() }, a.offset(), Some(a))
        }
        Placement::Placed(heap, offset) => {
            check_placement(heap, offset, req, &desc.label)?;
            // SAFETY: reading the heap's memory handle to bind it.
            (
                unsafe { heap.allocation.memory() },
                heap.allocation.offset() + offset,
                None,
            )
        }
    };
    // SAFETY: the memory satisfies `req` (allocator, or `check_placement`) and is bound once.
    match unsafe { ctx.device.bind_image_memory(raw, memory, offset) } {
        Ok(()) => Ok(own),
        Err(e) => {
            if let Some(a) = own {
                let _ = ctx.allocator.free(a);
            }
            Err(error("vkBindImageMemory", e))
        }
    }
}

fn check_placement(
    heap: &VkHeap,
    offset: u64,
    req: vk::MemoryRequirements,
    label: &str,
) -> Result<(), RhiError> {
    let bad = |reason: String| {
        Err(RhiError::invalid(
            "placed texture",
            format!("{label:?}: {reason}"),
        ))
    };
    if req.memory_type_bits & heap.memory_type_bits != heap.memory_type_bits {
        return Err(RhiError::Unsupported(format!(
            "{label:?} can't live in a heap on this device (memory types {:#x} vs the heap's {:#x})",
            req.memory_type_bits, heap.memory_type_bits
        )));
    }
    if !(heap.allocation.offset() + offset).is_multiple_of(req.alignment.max(1)) {
        return bad(format!(
            "offset {offset} is not aligned to {}",
            req.alignment
        ));
    }
    match offset.checked_add(req.size) {
        Some(end) if end <= heap.size => Ok(()),
        _ => bad(format!(
            "{} bytes at {offset} overflow the {}-byte heap",
            req.size, heap.size
        )),
    }
}

/// Whether a texture of `usage` gets image views (copy-only textures may not have any).
pub(crate) fn has_views(usage: concordia_rhi::TextureUsage) -> bool {
    use concordia_rhi::TextureUsage as U;
    usage.intersects(U::SAMPLED | U::STORAGE | U::RENDER_TARGET)
}

fn create_views(
    device: &ash::Device,
    raw: vk::Image,
    desc: &TextureDesc,
) -> Result<(vk::ImageView, vk::ImageView), RhiError> {
    if !has_views(desc.usage) {
        return Ok((vk::ImageView::null(), vk::ImageView::null()));
    }
    let view = create_view(device, raw, desc, desc.mip_levels.get())?;
    if desc.mip_levels.get() == 1 {
        return Ok((view, view));
    }
    match create_view(device, raw, desc, 1) {
        Ok(mip0) => Ok((view, mip0)),
        Err(e) => {
            // SAFETY: `view` is unused.
            unsafe { device.destroy_image_view(view, None) };
            Err(e)
        }
    }
}

fn create_view(
    device: &ash::Device,
    raw: vk::Image,
    desc: &TextureDesc,
    mips: u32,
) -> Result<vk::ImageView, RhiError> {
    let info = vk::ImageViewCreateInfo::default()
        .image(raw)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(convert::format(desc.format))
        .subresource_range(range(desc, mips));
    // SAFETY: `raw` is a live image with bound memory; the range lies within it.
    unsafe { device.create_image_view(&info, None) }.map_err(|e| error("vkCreateImageView", e))
}

/// The first `mips` levels of a texture's single layer.
pub(crate) fn range(desc: &TextureDesc, mips: u32) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(convert::aspect(desc.format))
        .base_mip_level(0)
        .level_count(mips)
        .base_array_layer(0)
        .layer_count(1)
}

/// Creates heap memory every common texture kind accepts.
pub(crate) fn create_heap(
    ctx: &mut Ctx<'_>,
    size: u64,
    label: &str,
    memory_type_bits: u32,
) -> Result<VkHeap, RhiError> {
    if memory_type_bits == 0 {
        return Err(RhiError::Unsupported(
            "no memory type holds every kind of placed texture".into(),
        ));
    }
    let requirements = vk::MemoryRequirements {
        size,
        alignment: 1 << 16,
        memory_type_bits,
    };
    let allocation = allocate(
        ctx,
        label,
        requirements,
        gpu_allocator::MemoryLocation::GpuOnly,
        false,
    )?;
    Ok(VkHeap {
        allocation,
        size,
        memory_type_bits,
    })
}

/// Frees an object the GPU has finished with.
pub(crate) fn free(ctx: &mut Ctx<'_>, dead: Dead) {
    match dead {
        Dead::Buffer(b) => free_buffer_parts(ctx, b.raw, b.allocation),
        Dead::Texture(t) if t.swapchain => {}
        Dead::Texture(t) => {
            // SAFETY: the GPU has finished with the texture (retired) and its views.
            unsafe {
                if t.mip0 != t.view {
                    ctx.device.destroy_image_view(t.mip0, None);
                }
                ctx.device.destroy_image_view(t.view, None);
                ctx.device.destroy_image(t.raw, None);
            }
            if let Some(a) = t.allocation {
                let _ = ctx.allocator.free(a);
            }
        }
        Dead::Heap(h) => {
            let _ = ctx.allocator.free(h.allocation);
        }
        // SAFETY: the GPU has finished with the pipeline (retired).
        Dead::Pipeline(p) => unsafe { ctx.device.destroy_pipeline(p, None) },
        // SAFETY: no submission waits on or signals the semaphore any more (retired).
        Dead::Semaphore(s) => unsafe { ctx.device.destroy_semaphore(s, None) },
    }
}

/// True when `usage` gets bindless slots (it must be sampled or storage).
pub(crate) fn bindless_texture(usage: TextureUsage) -> Result<bool, RhiError> {
    if !usage.contains(TextureUsage::BINDLESS) {
        return Ok(false);
    }
    match usage.intersects(TextureUsage::SAMPLED | TextureUsage::STORAGE) {
        true => Ok(true),
        false => Err(RhiError::invalid(
            "texture desc",
            "BINDLESS textures must be SAMPLED or STORAGE",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concordia_rhi::TextureUsage;

    #[test]
    fn only_viewable_usages_get_image_views() {
        for usage in [
            TextureUsage::SAMPLED,
            TextureUsage::STORAGE,
            TextureUsage::RENDER_TARGET,
            TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC,
            TextureUsage::SAMPLED | TextureUsage::COPY_DST | TextureUsage::BINDLESS,
        ] {
            assert!(has_views(usage), "{usage:?}");
        }
        for usage in [
            TextureUsage::COPY_SRC,
            TextureUsage::COPY_DST,
            TextureUsage::COPY_SRC | TextureUsage::COPY_DST,
        ] {
            assert!(!has_views(usage), "{usage:?}");
        }
    }
}
