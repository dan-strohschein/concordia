//! PNG, JPEG and Radiance HDR → `TextureData` with a box-filtered mip chain.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::data::{TextureData, TextureFormat};
use crate::error::AssetError;
use crate::importer::{Artifact, AssetKind, ImportInput, ImportOutput, Importer, encode};

/// How 8-bit pixels are encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum ColorSpace {
    /// Color data (base color, emissive).
    #[default]
    Srgb,
    /// Non-color data (normals, roughness, masks).
    Linear,
}

/// Image import settings (in the sidecar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct ImageSettings {
    /// Color space of 8-bit images (HDR is always linear float).
    pub color_space: ColorSpace,
    /// Build the full mip chain.
    pub generate_mips: bool,
    /// Treat as a normal map (linear, renormalized mips).
    pub normal_map: bool,
}

impl Default for ImageSettings {
    fn default() -> Self {
        Self {
            color_space: ColorSpace::Srgb,
            generate_mips: true,
            normal_map: false,
        }
    }
}

/// The image importer.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageImporter;

impl Importer for ImageImporter {
    fn name(&self) -> &'static str {
        "image"
    }

    fn version(&self) -> u32 {
        1
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["png", "jpg", "jpeg", "hdr"]
    }

    fn default_settings(&self) -> serde_json::Value {
        serde_json::to_value(ImageSettings::default()).unwrap_or_default()
    }

    fn import(&self, input: ImportInput<'_>) -> Result<ImportOutput, AssetError> {
        let settings: ImageSettings =
            serde_json::from_value(input.settings.clone()).map_err(|e| AssetError::Import {
                path: input.path.to_path_buf(),
                reason: format!("settings: {e}"),
            })?;
        let texture = decode(input.bytes, &settings).map_err(|reason| AssetError::Import {
            path: input.path.to_path_buf(),
            reason,
        })?;
        let mut output = ImportOutput::default();
        output.artifacts.insert(
            String::new(),
            Artifact {
                kind: AssetKind::Texture,
                bytes: encode(&texture)?,
            },
        );
        Ok(output)
    }
}

/// Decodes image bytes into a texture (used by the glTF importer for embedded images too).
pub fn decode(bytes: &[u8], settings: &ImageSettings) -> Result<TextureData, String> {
    let image = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let (width, height) = (image.width(), image.height());
    let hdr = matches!(
        image.color(),
        image::ColorType::Rgb32F | image::ColorType::Rgba32F
    );
    let (format, mip0) = match hdr {
        true => (
            TextureFormat::Rgba16Float,
            half_bytes(&image.to_rgba32f().into_raw()),
        ),
        false => {
            let linear = settings.normal_map || settings.color_space == ColorSpace::Linear;
            (
                if linear {
                    TextureFormat::Rgba8Linear
                } else {
                    TextureFormat::Rgba8Srgb
                },
                image.to_rgba8().into_raw(),
            )
        }
    };
    let mut mips = vec![mip0];
    if settings.generate_mips && !hdr {
        let (mut w, mut h) = (width, height);
        while w > 1 || h > 1 {
            let next = downsample(
                mips.last().map(Vec::as_slice).unwrap_or_default(),
                w,
                h,
                settings.normal_map,
            );
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            mips.push(next);
        }
    }
    Ok(TextureData {
        width,
        height,
        format,
        mips,
    })
}

/// 2×2 box filter of RGBA8 (normal maps renormalized).
fn downsample(src: &[u8], w: u32, h: u32, normal: bool) -> Vec<u8> {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let px = |x: u32, y: u32, c: usize| {
        u32::from(src[((y.min(h - 1) * w + x.min(w - 1)) * 4) as usize + c])
    };
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        for x in 0..nw {
            let mut texel: [u8; 4] = std::array::from_fn(|c| {
                let sum = px(2 * x, 2 * y, c)
                    + px(2 * x + 1, 2 * y, c)
                    + px(2 * x, 2 * y + 1, c)
                    + px(2 * x + 1, 2 * y + 1, c);
                ((sum + 2) / 4) as u8
            });
            if normal {
                renormalize(&mut texel);
            }
            out.extend_from_slice(&texel);
        }
    }
    out
}

fn renormalize(texel: &mut [u8; 4]) {
    let v: [f32; 3] = std::array::from_fn(|i| f32::from(texel[i]) / 127.5 - 1.0);
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    for i in 0..3 {
        texel[i] = ((v[i] / len + 1.0) * 127.5).round().clamp(0.0, 255.0) as u8;
    }
}

/// f32 RGBA → IEEE half RGBA bytes (round to nearest even via f32 bit manipulation).
fn half_bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|v| f32_to_f16(*v).to_le_bytes())
        .collect()
}

fn f32_to_f16(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let mant = bits & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = (mant | 0x80_0000) >> (1 - e);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    let half = sign | ((e as u16) << 10) | (mant >> 13) as u16;
    half + ((mant & 0x1fff) > 0x1000 || (mant & 0x3fff) == 0x3000) as u16
}
