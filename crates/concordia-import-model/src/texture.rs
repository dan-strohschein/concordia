//! How a material uses an image, which decides how it is decoded.

use concordia_assets::{ColorSpace, ImageSettings};

/// A texture's role: color (sRGB), data (linear) or a normal map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureUsage {
    /// Base color and emissive (sRGB).
    Color,
    /// Masks such as metallic-roughness and occlusion (linear).
    Data,
    /// Normal maps (linear, renormalized mips).
    Normal,
}

impl TextureUsage {
    /// Decode settings for this usage (always with mips).
    pub fn settings(self) -> ImageSettings {
        ImageSettings {
            color_space: match self {
                Self::Color => ColorSpace::Srgb,
                Self::Data | Self::Normal => ColorSpace::Linear,
            },
            generate_mips: true,
            normal_map: self == Self::Normal,
        }
    }
}
