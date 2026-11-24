//! Images in method results; MCP adapters return fields of this type as image content.

use base64::Engine as _;
use concordia_reflect::{Reflect, Shape, TypeInfo};
use serde::{Deserialize, Serialize};

/// An encoded image (PNG in M1) carried as base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct EncodedImage {
    /// Media type, e.g. `image/png`.
    pub mime_type: String,
    /// The encoded bytes, base64 (standard alphabet, padded).
    pub data_base64: String,
}

impl EncodedImage {
    /// A PNG of `width × height` RGBA8 pixels (rows from the top).
    pub fn from_rgba8(width: u32, height: u32, rgba: &[u8]) -> Result<Self, String> {
        if rgba.len() != width as usize * height as usize * 4 {
            return Err(format!(
                "{width}×{height} needs {} bytes, got {}",
                width as usize * height as usize * 4,
                rgba.len()
            ));
        }
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let fail = |e: png::EncodingError| format!("PNG encoding failed: {e}");
        let mut writer = encoder.write_header().map_err(fail)?;
        writer.write_image_data(rgba).map_err(fail)?;
        writer.finish().map_err(fail)?;
        Ok(Self::png(&out))
    }

    /// A PNG from its encoded bytes.
    pub fn png(bytes: &[u8]) -> Self {
        Self {
            mime_type: "image/png".into(),
            data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }
}

/// Names of a result type's top-level fields that hold an [`EncodedImage`].
pub fn image_fields(result: &TypeInfo) -> Vec<&'static str> {
    let image = EncodedImage::type_info();
    match &result.shape {
        Shape::Struct { fields, .. } => fields
            .iter()
            .filter(|f| {
                let info = f.ty.info();
                info.name == image.name && info.rust_path == image.rust_path
            })
            .map(|f| f.name)
            .collect(),
        _ => Vec::new(),
    }
}
