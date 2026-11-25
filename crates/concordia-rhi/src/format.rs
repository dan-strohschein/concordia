//! Texture formats.

/// A texture format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// 8-bit red, normalized.
    R8Unorm,
    /// 8-bit RGBA, normalized, linear.
    Rgba8Unorm,
    /// 8-bit RGBA, normalized, sRGB-encoded.
    Rgba8UnormSrgb,
    /// 8-bit BGRA, normalized, linear (swapchains).
    Bgra8Unorm,
    /// 8-bit BGRA, sRGB-encoded (swapchains).
    Bgra8UnormSrgb,
    /// 16-bit float RGBA (HDR targets).
    Rgba16Float,
    /// 16-bit float red and green (motion vectors).
    Rg16Float,
    /// 32-bit float RGBA.
    Rgba32Float,
    /// 32-bit float red.
    R32Float,
    /// 32-bit unsigned red.
    R32Uint,
    /// 64-bit unsigned red (visibility buffer; needs `RhiCaps` 64-bit atomics).
    R64Uint,
    /// 32-bit float depth.
    Depth32Float,
}

impl Format {
    /// Bytes per pixel (all M1 formats are uncompressed).
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::R8Unorm => 1,
            Self::Rgba8Unorm
            | Self::Rgba8UnormSrgb
            | Self::Bgra8Unorm
            | Self::Bgra8UnormSrgb
            | Self::R32Float
            | Self::R32Uint
            | Self::Rg16Float
            | Self::Depth32Float => 4,
            Self::Rgba16Float | Self::R64Uint => 8,
            Self::Rgba32Float => 16,
        }
    }

    /// True for depth formats.
    pub fn is_depth(self) -> bool {
        self == Self::Depth32Float
    }

    /// True for sRGB-encoded formats.
    pub fn is_srgb(self) -> bool {
        matches!(self, Self::Rgba8UnormSrgb | Self::Bgra8UnormSrgb)
    }
}
