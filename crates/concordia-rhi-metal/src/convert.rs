//! RHI enums → Metal enums.

use concordia_rhi::{
    BlendMode, CompareOp, CullMode, Format, MemoryLocation, RhiError, TextureUsage,
};
use objc2_metal::{
    MTLBlendFactor, MTLCompareFunction, MTLCullMode, MTLPixelFormat, MTLResourceOptions,
    MTLStorageMode, MTLTextureUsage,
};

pub(crate) fn pixel_format(format: Format) -> Result<MTLPixelFormat, RhiError> {
    Ok(match format {
        Format::R8Unorm => MTLPixelFormat::R8Unorm,
        Format::Rgba8Unorm => MTLPixelFormat::RGBA8Unorm,
        Format::Rgba8UnormSrgb => MTLPixelFormat::RGBA8Unorm_sRGB,
        Format::Bgra8Unorm => MTLPixelFormat::BGRA8Unorm,
        Format::Bgra8UnormSrgb => MTLPixelFormat::BGRA8Unorm_sRGB,
        Format::Rgba16Float => MTLPixelFormat::RGBA16Float,
        Format::Rgba32Float => MTLPixelFormat::RGBA32Float,
        Format::R32Float => MTLPixelFormat::R32Float,
        Format::R32Uint => MTLPixelFormat::R32Uint,
        Format::Depth32Float => MTLPixelFormat::Depth32Float,
        Format::R64Uint => {
            return Err(RhiError::Unsupported(
                "R64Uint textures on the Metal 3 backend".into(),
            ));
        }
    })
}

pub(crate) fn texture_usage(usage: TextureUsage) -> MTLTextureUsage {
    let mut out = MTLTextureUsage::Unknown;
    if usage.intersects(TextureUsage::SAMPLED | TextureUsage::STORAGE) {
        out |= MTLTextureUsage::ShaderRead;
    }
    if usage.contains(TextureUsage::STORAGE) {
        out |= MTLTextureUsage::ShaderWrite;
    }
    if usage.contains(TextureUsage::RENDER_TARGET) {
        out |= MTLTextureUsage::RenderTarget;
    }
    out
}

pub(crate) fn buffer_options(memory: MemoryLocation) -> MTLResourceOptions {
    let storage = match memory {
        MemoryLocation::GpuOnly => MTLResourceOptions::StorageModePrivate,
        MemoryLocation::Upload | MemoryLocation::Readback => MTLResourceOptions::StorageModeShared,
    };
    storage | MTLResourceOptions::HazardTrackingModeTracked
}

pub(crate) fn private_storage() -> MTLStorageMode {
    MTLStorageMode::Private
}

pub(crate) fn compare(op: CompareOp) -> MTLCompareFunction {
    match op {
        CompareOp::Never => MTLCompareFunction::Never,
        CompareOp::Less => MTLCompareFunction::Less,
        CompareOp::LessEqual => MTLCompareFunction::LessEqual,
        CompareOp::Greater => MTLCompareFunction::Greater,
        CompareOp::GreaterEqual => MTLCompareFunction::GreaterEqual,
        CompareOp::Always => MTLCompareFunction::Always,
    }
}

pub(crate) fn cull(mode: CullMode) -> MTLCullMode {
    match mode {
        CullMode::None => MTLCullMode::None,
        CullMode::Back => MTLCullMode::Back,
        CullMode::Front => MTLCullMode::Front,
    }
}

/// (source, destination) factors for color and alpha, or `None` for no blending.
pub(crate) fn blend(mode: BlendMode) -> Option<(MTLBlendFactor, MTLBlendFactor)> {
    match mode {
        BlendMode::Opaque => None,
        BlendMode::PremultipliedAlpha => {
            Some((MTLBlendFactor::One, MTLBlendFactor::OneMinusSourceAlpha))
        }
        BlendMode::Additive => Some((MTLBlendFactor::One, MTLBlendFactor::One)),
    }
}
