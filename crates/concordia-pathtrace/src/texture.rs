//! Textures as the tracer samples them: linear RGBA, bilinear with repeat, texel centres at
//! half-texels and rows from the top (the renderer's `SAMPLER_LINEAR_REPEAT` at mip 0).

use concordia_assets::TextureData;
use concordia_core::math::Vec2;

use crate::PathTraceError;

/// A decoded texture.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneTexture {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Linear RGBA, rows from the top.
    pub texels: Vec<[f32; 4]>,
}

impl SceneTexture {
    /// Decodes mip 0 of `data`.
    pub fn new(data: &TextureData) -> Result<Self, PathTraceError> {
        if data.width == 0 || data.height == 0 {
            return Err(PathTraceError::Texture(
                "a texture needs at least one texel".into(),
            ));
        }
        let texels = data.linear_rgba().map_err(PathTraceError::Texture)?;
        Ok(Self {
            width: data.width,
            height: data.height,
            texels,
        })
    }

    /// The texture at `uv`: bilinear between the four nearest texel centres, wrapping.
    pub fn sample(&self, uv: Vec2) -> [f32; 4] {
        let (x, fx) = split(uv.x * self.width as f32 - 0.5);
        let (y, fy) = split(uv.y * self.height as f32 - 0.5);
        let at = |dx: i64, dy: i64| {
            let col = (x + dx).rem_euclid(i64::from(self.width)) as usize;
            let row = (y + dy).rem_euclid(i64::from(self.height)) as usize;
            self.texels[row * self.width as usize + col]
        };
        let lerp =
            |a: [f32; 4], b: [f32; 4], t: f32| [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t);
        lerp(
            lerp(at(0, 0), at(1, 0), fx),
            lerp(at(0, 1), at(1, 1), fx),
            fy,
        )
    }
}

/// The texel at or below `x` and the fraction past it (non-finite coordinates read texel 0).
fn split(x: f32) -> (i64, f32) {
    if !x.is_finite() {
        return (0, 0.0);
    }
    let floor = x.floor();
    (floor as i64, x - floor)
}
