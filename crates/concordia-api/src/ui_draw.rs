//! The in-game UI's draw list (ADR-0060): quads in target pixels that a module builds on the
//! engine thread and the renderer draws in one batched pass after tonemapping.

use std::sync::Arc;

/// The debug layer that outlines every UI node (drawn by the UI module, not as geometry).
pub const UI_BOUNDS_LAYER: &str = "ui_bounds";

/// What the UI is drawn into: the target's pixel size and its pixels per logical pixel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixels per logical pixel (the window's scale factor; 1 for captures).
    pub scale: f32,
    /// Outline every node (the `ui_bounds` debug layer).
    pub bounds: bool,
}

impl UiFrame {
    /// A frame of `width` × `height` pixels at scale 1.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            scale: 1.0,
            bounds: false,
        }
    }

    /// The viewport in logical pixels.
    pub fn logical(&self) -> [f32; 2] {
        let scale = if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        };
        [self.width as f32 / scale, self.height as f32 / scale]
    }
}

/// Every quad to draw, in order, plus the textures they sample.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiDrawList {
    /// Quads, back to front.
    pub quads: Vec<UiQuad>,
    /// Textures the quads name by index.
    pub textures: Vec<UiTexture>,
}

impl UiDrawList {
    /// Whether there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    /// Appends `other`, re-indexing its textures.
    pub fn append(&mut self, mut other: UiDrawList) {
        let base = self.textures.len() as u32;
        for quad in &mut other.quads {
            if let UiPaint::Texture { texture, .. } | UiPaint::Glyph { texture, .. } =
                &mut quad.paint
            {
                *texture += base;
            }
        }
        self.quads.append(&mut other.quads);
        self.textures.append(&mut other.textures);
    }
}

/// One rectangle: its paint, rounded corners, border, clip and an optional fill mask.
#[derive(Debug, Clone, PartialEq)]
pub struct UiQuad {
    /// `[x, y, w, h]` in target pixels (origin top left, y down).
    pub rect: [f32; 4],
    /// `[x0, y0, x1, y1]`: pixels outside are discarded.
    pub clip: [f32; 4],
    /// Corner radii `[top left, top right, bottom right, bottom left]` in pixels.
    pub radius: [f32; 4],
    /// Border width in pixels (drawn inside the rect).
    pub border: f32,
    /// Linear RGBA border color.
    pub border_color: [f32; 4],
    /// What fills the inside.
    pub paint: UiPaint,
    /// Which part of the quad shows.
    pub mask: UiMask,
    /// Multiplies the whole quad's alpha.
    pub opacity: f32,
}

impl UiQuad {
    /// A quad of `paint` over `rect`, unclipped, square, borderless and opaque.
    pub fn new(rect: [f32; 4], paint: UiPaint) -> Self {
        Self {
            rect,
            clip: [f32::MIN, f32::MIN, f32::MAX, f32::MAX],
            radius: [0.0; 4],
            border: 0.0,
            border_color: [0.0; 4],
            paint,
            mask: UiMask::None,
            opacity: 1.0,
        }
    }
}

/// A quad's fill.
#[derive(Debug, Clone, PartialEq)]
pub enum UiPaint {
    /// Nothing (a border only).
    None,
    /// One linear RGBA color.
    Solid([f32; 4]),
    /// A linear gradient from `from` to `to` (pixels).
    Linear {
        /// Where offset 0 lies.
        from: [f32; 2],
        /// Where offset 1 lies.
        to: [f32; 2],
        /// 2–8 stops, ascending.
        stops: Vec<UiStop>,
    },
    /// A radial gradient: offset 1 at `radius` (pixels, per axis) from `center`.
    Radial {
        /// The center in pixels.
        center: [f32; 2],
        /// Radii in pixels.
        radius: [f32; 2],
        /// 2–8 stops, ascending.
        stops: Vec<UiStop>,
    },
    /// A texture's `uv` rect (`[u0, v0, u1, v1]`), multiplied by `tint`.
    Texture {
        /// Index into [`UiDrawList::textures`].
        texture: u32,
        /// The sampled rect.
        uv: [f32; 4],
        /// Linear RGBA multiplier.
        tint: [f32; 4],
    },
    /// A glyph: the atlas's coverage at `uv` times `color`.
    Glyph {
        /// Index into [`UiDrawList::textures`] (an `R8` atlas page).
        texture: u32,
        /// The glyph's rect in the atlas.
        uv: [f32; 4],
        /// Linear RGBA color.
        color: [f32; 4],
    },
}

/// A gradient stop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiStop {
    /// Position along the gradient, 0..1.
    pub offset: f32,
    /// Linear RGBA.
    pub color: [f32; 4],
}

/// Which part of a quad shows (progress fills).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum UiMask {
    /// All of it.
    #[default]
    None,
    /// The pixels whose angle about `center` lies within `cw` radians clockwise or `ccw`
    /// radians counterclockwise of `start` (radians, 0 is up, clockwise positive).
    Angular {
        /// The pivot in pixels.
        center: [f32; 2],
        /// The starting angle.
        start: f32,
        /// Extent clockwise.
        cw: f32,
        /// Extent counterclockwise.
        ccw: f32,
    },
}

/// A texture's pixel format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiTextureFormat {
    /// sRGB RGBA8 (images).
    Rgba8Srgb,
    /// Linear RGBA8.
    Rgba8Linear,
    /// One coverage byte per pixel (glyph atlases).
    R8,
}

/// A texture the UI samples, cached by the renderer under `(key, generation)`.
#[derive(Debug, Clone, PartialEq)]
pub struct UiTexture {
    /// Stable identity (an image's asset, an atlas page).
    pub key: u64,
    /// Changes whenever the pixels do.
    pub generation: u64,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The format.
    pub format: UiTextureFormat,
    /// Rows top to bottom.
    pub pixels: Arc<[u8]>,
}
