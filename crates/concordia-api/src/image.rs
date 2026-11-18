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
