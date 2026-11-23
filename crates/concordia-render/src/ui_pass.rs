//! The UI pass's data (ADR-0060): each `UiQuad` packed into 40 floats and each gradient stop into
//! 8, read by `ui.slang` through bindless buffers; and the UI's textures, cached on the GPU by
//! `(key, generation)`.

use std::collections::BTreeMap;

use concordia_api::{UiDrawList, UiMask, UiPaint, UiQuad, UiStop};
use concordia_assets::{TextureData, TextureFormat};
use concordia_rhi::{Device, Texture};

use crate::gpu::Uploads;

/// Floats per quad.
pub const UI_QUAD_FLOATS: usize = 40;
/// Floats per gradient stop.
pub const UI_STOP_FLOATS: usize = 8;

/// Push constants for the UI draw (mirrors `UiPush` in `ui.slang`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UiPush {
    /// Bindless index of the quads.
    pub quads: u32,
    /// Bindless index of the stops.
    pub stops: u32,
    /// Target width in pixels.
    pub width: f32,
    /// Target height in pixels.
    pub height: f32,
}

concordia_shaders::shader_struct!(UiPush {
    quads,
    stops,
    width,
    height
});

impl UiPush {
    /// The push-constant bytes.
    pub fn bytes(&self) -> Vec<u8> {
        [
            self.quads.to_le_bytes(),
            self.stops.to_le_bytes(),
            self.width.to_le_bytes(),
            self.height.to_le_bytes(),
        ]
        .concat()
    }
}

/// Packs `list`'s quads and stops; `textures` is each list texture's bindless index.
pub fn encode_ui(list: &UiDrawList, textures: &[u32]) -> (Vec<f32>, Vec<f32>) {
    let mut quads = Vec::with_capacity(list.quads.len() * UI_QUAD_FLOATS);
    let mut stops = Vec::new();
    for quad in &list.quads {
        quads.extend(encode_quad(quad, textures, &mut stops));
    }
    (quads, stops)
}

/// One quad's 40 floats, appending its gradient stops to `stops`.
fn encode_quad(q: &UiQuad, textures: &[u32], stops: &mut Vec<f32>) -> [f32; UI_QUAD_FLOATS] {
    let mut f = [0.0f32; UI_QUAD_FLOATS];
    f[0..4].copy_from_slice(&q.rect);
    f[4..8].copy_from_slice(&q.clip);
    f[8..12].copy_from_slice(&q.radius);
    f[12..16].copy_from_slice(&q.border_color);
    f[33] = q.border;
    f[34] = q.opacity;
    let bindless = |t: u32| textures.get(t as usize).copied().unwrap_or(0) as f32;
    let mut gradient =
        |f: &mut [f32; UI_QUAD_FLOATS], kind: f32, a: [f32; 2], b: [f32; 2], list: &[UiStop]| {
            f[24..26].copy_from_slice(&a);
            f[26..28].copy_from_slice(&b);
            f[35] = kind;
            f[37] = (stops.len() / UI_STOP_FLOATS) as f32;
            f[38] = list.len() as f32;
            for stop in list {
                stops.extend([stop.offset, 0.0, 0.0, 0.0]);
                stops.extend(stop.color);
            }
        };
    match &q.paint {
        UiPaint::None => {}
        UiPaint::Solid(color) => {
            f[16..20].copy_from_slice(color);
            f[35] = 1.0;
        }
        UiPaint::Linear { from, to, stops } => gradient(&mut f, 2.0, *from, *to, stops),
        UiPaint::Radial {
            center,
            radius,
            stops,
        } => gradient(&mut f, 3.0, *center, *radius, stops),
        UiPaint::Texture { texture, uv, tint } => {
            f[16..20].copy_from_slice(tint);
            f[20..24].copy_from_slice(uv);
            f[35] = 4.0;
            f[36] = bindless(*texture);
        }
        UiPaint::Glyph { texture, uv, color } => {
            f[16..20].copy_from_slice(color);
            f[20..24].copy_from_slice(uv);
            f[35] = 5.0;
            f[36] = bindless(*texture);
        }
    }
    if let UiMask::Angular {
        center,
        start,
        cw,
        ccw,
    } = q.mask
    {
        f[28..32].copy_from_slice(&[center[0], center[1], start, cw]);
        f[32] = ccw;
        f[39] = 1.0;
    }
    f
}

/// The UI's textures on the GPU, by key: (generation, texture, bindless index).
#[derive(Debug, Default)]
pub(crate) struct UiTextures {
    cached: BTreeMap<u64, (u64, Texture, u32)>,
}

impl UiTextures {
    /// Each of `list`'s textures' bindless index, uploading new or changed ones through `up`;
    /// replaced textures go to `retired`.
    pub(crate) fn prepare(
        &mut self,
        device: &dyn Device,
        up: &mut Uploads,
        list: &UiDrawList,
        retired: &mut Vec<Texture>,
    ) -> Result<Vec<u32>, crate::RenderError> {
        let mut out = Vec::with_capacity(list.textures.len());
        for t in &list.textures {
            if let Some((generation, _, index)) = self.cached.get(&t.key)
                && *generation == t.generation
            {
                out.push(*index);
                continue;
            }
            let data = TextureData {
                width: t.width,
                height: t.height,
                format: match t.format {
                    concordia_api::UiTextureFormat::Rgba8Srgb => TextureFormat::Rgba8Srgb,
                    concordia_api::UiTextureFormat::Rgba8Linear => TextureFormat::Rgba8Linear,
                    concordia_api::UiTextureFormat::R8 => TextureFormat::R8,
                },
                mips: vec![t.pixels.to_vec()],
            };
            let texture = up.texture(device, &data, "ui texture")?;
            let index = device.texture_index(texture)?.0;
            if let Some((_, old, _)) = self.cached.insert(t.key, (t.generation, texture, index)) {
                retired.push(old);
            }
            out.push(index);
        }
        Ok(out)
    }

    /// Frees every cached texture.
    pub(crate) fn destroy(&mut self, device: &dyn Device) {
        for (_, (_, texture, _)) in std::mem::take(&mut self.cached) {
            device.destroy_texture(texture);
        }
    }
}
