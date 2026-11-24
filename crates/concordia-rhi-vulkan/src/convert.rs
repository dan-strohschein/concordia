//! RHI values → Vulkan values (pure).

use ash::vk;
use concordia_rhi::{
    BlendMode, BufferUsage, ClearValue, CompareOp, CullMode, Format, Layout, LoadOp,
    MemoryLocation, RhiError, SampleCount, StaticSampler, StoreOp, TextureUsage,
};

/// Every triangle that is counter-clockwise in clip space (Y up) is front-facing, as on Metal.
/// With the negative-height viewport, that is Vulkan's `COUNTER_CLOCKWISE` (ADR-0088).
pub(crate) const FRONT_FACE: vk::FrontFace = vk::FrontFace::COUNTER_CLOCKWISE;

/// The pixel format.
pub(crate) fn format(format: Format) -> vk::Format {
    match format {
        Format::R8Unorm => vk::Format::R8_UNORM,
        Format::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
        Format::Rgba8UnormSrgb => vk::Format::R8G8B8A8_SRGB,
        Format::Bgra8Unorm => vk::Format::B8G8R8A8_UNORM,
        Format::Bgra8UnormSrgb => vk::Format::B8G8R8A8_SRGB,
        Format::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
        Format::Rgba32Float => vk::Format::R32G32B32A32_SFLOAT,
        Format::R32Float => vk::Format::R32_SFLOAT,
        Format::R32Uint => vk::Format::R32_UINT,
        Format::R64Uint => vk::Format::R64_UINT,
        Format::Depth32Float => vk::Format::D32_SFLOAT,
    }
}

/// The aspect a format's views and copies address.
pub(crate) fn aspect(format: Format) -> vk::ImageAspectFlags {
    match format.is_depth() {
        true => vk::ImageAspectFlags::DEPTH,
        false => vk::ImageAspectFlags::COLOR,
    }
}

/// Buffer usage. Storage and bindless buffers are reached through device addresses.
pub(crate) fn buffer_usage(usage: BufferUsage) -> vk::BufferUsageFlags {
    let mut flags = vk::BufferUsageFlags::empty();
    for (rhi, vk_flags) in [
        (
            BufferUsage::STORAGE,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        ),
        (BufferUsage::INDEX, vk::BufferUsageFlags::INDEX_BUFFER),
        (BufferUsage::INDIRECT, vk::BufferUsageFlags::INDIRECT_BUFFER),
        (BufferUsage::COPY_SRC, vk::BufferUsageFlags::TRANSFER_SRC),
        (BufferUsage::COPY_DST, vk::BufferUsageFlags::TRANSFER_DST),
        (
            BufferUsage::BINDLESS,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        ),
    ] {
        if usage.contains(rhi) {
            flags |= vk_flags;
        }
    }
    flags
}

/// Image usage; render targets become color or depth attachments by format.
pub(crate) fn texture_usage(usage: TextureUsage, format: Format) -> vk::ImageUsageFlags {
    let attachment = match format.is_depth() {
        true => vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
        false => vk::ImageUsageFlags::COLOR_ATTACHMENT,
    };
    let mut flags = vk::ImageUsageFlags::empty();
    for (rhi, vk_flags) in [
        (TextureUsage::SAMPLED, vk::ImageUsageFlags::SAMPLED),
        (TextureUsage::STORAGE, vk::ImageUsageFlags::STORAGE),
        (TextureUsage::RENDER_TARGET, attachment),
        (TextureUsage::COPY_SRC, vk::ImageUsageFlags::TRANSFER_SRC),
        (TextureUsage::COPY_DST, vk::ImageUsageFlags::TRANSFER_DST),
    ] {
        if usage.contains(rhi) {
            flags |= vk_flags;
        }
    }
    flags
}

/// The format features `usage` needs.
pub(crate) fn format_features(usage: TextureUsage, format: Format) -> vk::FormatFeatureFlags {
    let attachment = match format.is_depth() {
        true => vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT,
        false => vk::FormatFeatureFlags::COLOR_ATTACHMENT,
    };
    let mut flags = vk::FormatFeatureFlags::empty();
    for (rhi, vk_flags) in [
        (TextureUsage::SAMPLED, vk::FormatFeatureFlags::SAMPLED_IMAGE),
        (TextureUsage::STORAGE, vk::FormatFeatureFlags::STORAGE_IMAGE),
        (TextureUsage::RENDER_TARGET, attachment),
        (TextureUsage::COPY_SRC, vk::FormatFeatureFlags::TRANSFER_SRC),
        (TextureUsage::COPY_DST, vk::FormatFeatureFlags::TRANSFER_DST),
    ] {
        if usage.contains(rhi) {
            flags |= vk_flags;
        }
    }
    flags
}

/// The image layout.
pub(crate) fn layout(layout: Layout) -> vk::ImageLayout {
    match layout {
        Layout::Undefined => vk::ImageLayout::UNDEFINED,
        Layout::General => vk::ImageLayout::GENERAL,
        Layout::ColorAttachment => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        Layout::DepthAttachment => vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
        Layout::ShaderRead => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        Layout::CopySrc => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        Layout::CopyDst => vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        Layout::Present => vk::ImageLayout::PRESENT_SRC_KHR,
    }
}

/// The sample count flag.
pub(crate) fn samples(count: SampleCount) -> vk::SampleCountFlags {
    vk::SampleCountFlags::from_raw(u32::from(count.get()))
}

/// The sample counts in a Vulkan mask, ascending.
pub(crate) fn sample_counts(mask: vk::SampleCountFlags) -> Vec<SampleCount> {
    SampleCount::ALL
        .into_iter()
        .filter(|c| mask.contains(samples(*c)))
        .collect()
}

/// The depth comparison.
pub(crate) fn compare(op: CompareOp) -> vk::CompareOp {
    match op {
        CompareOp::Never => vk::CompareOp::NEVER,
        CompareOp::Less => vk::CompareOp::LESS,
        CompareOp::LessEqual => vk::CompareOp::LESS_OR_EQUAL,
        CompareOp::Greater => vk::CompareOp::GREATER,
        CompareOp::GreaterEqual => vk::CompareOp::GREATER_OR_EQUAL,
        CompareOp::Always => vk::CompareOp::ALWAYS,
    }
}

/// Face culling.
pub(crate) fn cull(mode: CullMode) -> vk::CullModeFlags {
    match mode {
        CullMode::None => vk::CullModeFlags::NONE,
        CullMode::Back => vk::CullModeFlags::BACK,
        CullMode::Front => vk::CullModeFlags::FRONT,
    }
}

/// One color target's blend state.
pub(crate) fn blend(mode: BlendMode) -> vk::PipelineColorBlendAttachmentState {
    let state = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA);
    let (src, dst) = match mode {
        BlendMode::Opaque => return state,
        BlendMode::PremultipliedAlpha => {
            (vk::BlendFactor::ONE, vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        }
        BlendMode::Additive => (vk::BlendFactor::ONE, vk::BlendFactor::ONE),
    };
    state
        .blend_enable(true)
        .src_color_blend_factor(src)
        .dst_color_blend_factor(dst)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(src)
        .dst_alpha_blend_factor(dst)
        .alpha_blend_op(vk::BlendOp::ADD)
}

/// A static sampler (`anisotropy`: the device's sampler anisotropy feature).
pub(crate) fn sampler(sampler: StaticSampler, anisotropy: bool) -> vk::SamplerCreateInfo<'static> {
    let (filter, mip) = match sampler.is_linear() {
        true => (vk::Filter::LINEAR, vk::SamplerMipmapMode::LINEAR),
        false => (vk::Filter::NEAREST, vk::SamplerMipmapMode::NEAREST),
    };
    let address = match sampler.repeats() {
        true => vk::SamplerAddressMode::REPEAT,
        false => vk::SamplerAddressMode::CLAMP_TO_EDGE,
    };
    let anisotropic = anisotropy && sampler == StaticSampler::LinearRepeat;
    vk::SamplerCreateInfo::default()
        .mag_filter(filter)
        .min_filter(filter)
        .mipmap_mode(mip)
        .address_mode_u(address)
        .address_mode_v(address)
        .address_mode_w(address)
        .anisotropy_enable(anisotropic)
        .max_anisotropy(if anisotropic { 8.0 } else { 1.0 })
        .min_lod(0.0)
        .max_lod(vk::LOD_CLAMP_NONE)
}

/// The attachment load op and clear value.
pub(crate) fn load(op: LoadOp) -> (vk::AttachmentLoadOp, vk::ClearValue) {
    match op {
        LoadOp::Load => (vk::AttachmentLoadOp::LOAD, vk::ClearValue::default()),
        LoadOp::DontCare => (vk::AttachmentLoadOp::DONT_CARE, vk::ClearValue::default()),
        LoadOp::Clear(value) => (vk::AttachmentLoadOp::CLEAR, clear(value)),
    }
}

fn clear(value: ClearValue) -> vk::ClearValue {
    match value {
        ClearValue::Color(float32) => vk::ClearValue {
            color: vk::ClearColorValue { float32 },
        },
        ClearValue::Depth(depth) => vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue { depth, stencil: 0 },
        },
    }
}

/// The attachment store op.
pub(crate) fn store(op: StoreOp) -> vk::AttachmentStoreOp {
    match op {
        StoreOp::Store => vk::AttachmentStoreOp::STORE,
        StoreOp::DontCare => vk::AttachmentStoreOp::DONT_CARE,
    }
}

/// Where gpu-allocator places a buffer.
pub(crate) fn memory(location: MemoryLocation) -> gpu_allocator::MemoryLocation {
    match location {
        MemoryLocation::GpuOnly => gpu_allocator::MemoryLocation::GpuOnly,
        MemoryLocation::Upload => gpu_allocator::MemoryLocation::CpuToGpu,
        MemoryLocation::Readback => gpu_allocator::MemoryLocation::GpuToCpu,
    }
}

/// A failed Vulkan call as an RHI error; `what` names the call.
pub(crate) fn error(what: &str, result: vk::Result) -> RhiError {
    match result {
        vk::Result::ERROR_OUT_OF_HOST_MEMORY | vk::Result::ERROR_OUT_OF_DEVICE_MEMORY => {
            RhiError::OutOfMemory(format!("{what} ({result})"))
        }
        vk::Result::ERROR_DEVICE_LOST => RhiError::DeviceLost(format!("{what} ({result})")),
        vk::Result::ERROR_FEATURE_NOT_PRESENT
        | vk::Result::ERROR_FORMAT_NOT_SUPPORTED
        | vk::Result::ERROR_EXTENSION_NOT_PRESENT
        | vk::Result::ERROR_LAYER_NOT_PRESENT => {
            RhiError::Unsupported(format!("{what} ({result})"))
        }
        other => RhiError::invalid("vulkan call", format!("{what} failed: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORMATS: [Format; 11] = [
        Format::R8Unorm,
        Format::Rgba8Unorm,
        Format::Rgba8UnormSrgb,
        Format::Bgra8Unorm,
        Format::Bgra8UnormSrgb,
        Format::Rgba16Float,
        Format::Rgba32Float,
        Format::R32Float,
        Format::R32Uint,
        Format::R64Uint,
        Format::Depth32Float,
    ];

    #[test]
    fn formats_map_one_to_one_with_matching_aspects() {
        let mapped: std::collections::BTreeSet<i32> =
            FORMATS.iter().map(|f| format(*f).as_raw()).collect();
        assert_eq!(mapped.len(), FORMATS.len());
        for f in FORMATS {
            assert_eq!(
                aspect(f) == vk::ImageAspectFlags::DEPTH,
                f.is_depth(),
                "{f:?}"
            );
        }
    }

    #[test]
    fn layouts_map_one_to_one() {
        let mapped: std::collections::BTreeSet<i32> =
            Layout::ALL.iter().map(|l| layout(*l).as_raw()).collect();
        assert_eq!(mapped.len(), Layout::ALL.len());
        assert_eq!(layout(Layout::Undefined), vk::ImageLayout::UNDEFINED);
    }

    #[test]
    fn render_targets_are_color_or_depth_attachments_by_format() {
        let rt = TextureUsage::RENDER_TARGET;
        assert_eq!(
            texture_usage(rt, Format::Rgba8Unorm),
            vk::ImageUsageFlags::COLOR_ATTACHMENT
        );
        assert_eq!(
            texture_usage(rt, Format::Depth32Float),
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
        );
        assert_eq!(
            format_features(rt | TextureUsage::SAMPLED, Format::Depth32Float),
            vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT
                | vk::FormatFeatureFlags::SAMPLED_IMAGE
        );
    }

    #[test]
    fn bindless_and_storage_buffers_have_device_addresses() {
        for usage in [BufferUsage::BINDLESS, BufferUsage::STORAGE] {
            assert!(buffer_usage(usage).contains(vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS));
        }
        assert_eq!(
            buffer_usage(BufferUsage::COPY_DST),
            vk::BufferUsageFlags::TRANSFER_DST
        );
    }

    #[test]
    fn sample_counts_round_trip() {
        for count in SampleCount::ALL {
            assert_eq!(sample_counts(samples(count)), [count]);
        }
        let mask = vk::SampleCountFlags::TYPE_1
            | vk::SampleCountFlags::TYPE_4
            | vk::SampleCountFlags::TYPE_16;
        assert_eq!(sample_counts(mask), [SampleCount::X1, SampleCount::X4]);
    }

    #[test]
    fn blending_matches_the_rhi_modes() {
        assert_eq!(blend(BlendMode::Opaque).blend_enable, vk::FALSE);
        let over = blend(BlendMode::PremultipliedAlpha);
        assert_eq!(
            (over.src_color_blend_factor, over.dst_color_blend_factor),
            (vk::BlendFactor::ONE, vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        );
        let add = blend(BlendMode::Additive);
        assert_eq!(
            (add.src_alpha_blend_factor, add.dst_alpha_blend_factor),
            (vk::BlendFactor::ONE, vk::BlendFactor::ONE)
        );
    }

    #[test]
    fn static_samplers_filter_and_address_as_named() {
        for s in StaticSampler::ALL {
            let info = sampler(s, true);
            assert_eq!(
                info.mag_filter == vk::Filter::LINEAR,
                s.is_linear(),
                "{s:?}"
            );
            assert_eq!(
                info.address_mode_u == vk::SamplerAddressMode::REPEAT,
                s.repeats(),
                "{s:?}"
            );
            assert_eq!(
                info.anisotropy_enable == vk::TRUE,
                s == StaticSampler::LinearRepeat,
                "{s:?}"
            );
        }
        assert_eq!(
            sampler(StaticSampler::LinearRepeat, false).anisotropy_enable,
            vk::FALSE
        );
    }

    #[test]
    fn clears_carry_their_values() {
        let (op, value) = load(LoadOp::Clear(ClearValue::Color([0.25, 0.5, 0.75, 1.0])));
        assert_eq!(op, vk::AttachmentLoadOp::CLEAR);
        // SAFETY: a color clear writes the `color` member.
        assert_eq!(unsafe { value.color.float32 }, [0.25, 0.5, 0.75, 1.0]);
        let (_, depth) = load(LoadOp::Clear(ClearValue::Depth(0.0)));
        // SAFETY: a depth clear writes the `depth_stencil` member.
        assert_eq!(unsafe { depth.depth_stencil.depth }, 0.0);
        assert_eq!(load(LoadOp::Load).0, vk::AttachmentLoadOp::LOAD);
    }

    #[test]
    fn errors_name_the_call_and_keep_their_kind() {
        assert!(
            matches!(error("x", vk::Result::ERROR_DEVICE_LOST), RhiError::DeviceLost(m) if m.contains('x'))
        );
        assert!(matches!(
            error("x", vk::Result::ERROR_OUT_OF_DEVICE_MEMORY),
            RhiError::OutOfMemory(_)
        ));
        assert!(matches!(
            error("x", vk::Result::ERROR_FORMAT_NOT_SUPPORTED),
            RhiError::Unsupported(_)
        ));
        assert!(matches!(
            error("x", vk::Result::ERROR_UNKNOWN),
            RhiError::Invalid { .. }
        ));
    }
}
