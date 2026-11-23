//! The glyph atlas: swash coverage masks shelf-packed into `R8` pages. Presentation data held
//! outside the world (ADR-0060); a page's generation changes whenever its pixels do.

use std::collections::HashMap;

use parley::FontData;
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Format, Vector};

/// The default page side.
pub const PAGE_SIZE: u32 = 1024;
/// Empty pixels around each glyph.
const PAD: u32 = 1;

/// Where a glyph's mask lives and how it sits on the pen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphSlot {
    /// The page.
    pub page: u32,
    /// Left, in page pixels.
    pub x: u32,
    /// Top, in page pixels.
    pub y: u32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
    /// The mask's left edge from the pen.
    pub left: i32,
    /// The mask's top edge above the baseline.
    pub top: i32,
}

/// One `R8` page.
#[derive(Debug, Clone)]
pub struct AtlasPage {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Coverage, rows top to bottom.
    pub pixels: Vec<u8>,
    /// Bumped on every write.
    pub generation: u64,
    shelves: Vec<Shelf>,
}

#[derive(Debug, Clone, Copy)]
struct Shelf {
    y: u32,
    height: u32,
    x: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    blob: u64,
    index: u32,
    glyph: u32,
    quarter_px: u32,
    sub: u8,
}

/// Rasterized glyphs, packed.
pub struct GlyphAtlas {
    page_size: u32,
    pages: Vec<AtlasPage>,
    slots: HashMap<Key, Option<GlyphSlot>>,
    context: ScaleContext,
}

impl std::fmt::Debug for GlyphAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlyphAtlas")
            .field("pages", &self.pages.len())
            .field("glyphs", &self.slots.len())
            .finish()
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new(PAGE_SIZE)
    }
}

impl GlyphAtlas {
    /// An empty atlas of `page_size` square pages (at least 16).
    pub fn new(page_size: u32) -> Self {
        Self {
            page_size: page_size.max(16),
            pages: Vec::new(),
            slots: HashMap::new(),
            context: ScaleContext::new(),
        }
    }

    /// The pages.
    pub fn pages(&self) -> &[AtlasPage] {
        &self.pages
    }

    /// How many glyphs are cached (empty ones included).
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether nothing is cached.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Glyph `id` of `font` at `size` pixels, shifted right by `subpixel` (0..1, quantized to
    /// quarters); `None` when it has no outline or doesn't fit a page.
    pub fn glyph(
        &mut self,
        font: &FontData,
        id: u32,
        size: f32,
        subpixel: f32,
    ) -> Option<GlyphSlot> {
        if !(size.is_finite() && size > 0.0) {
            return None;
        }
        let sub = (subpixel.clamp(0.0, 0.999) * 4.0).floor() as u8;
        let key = Key {
            blob: font.data.id(),
            index: font.index,
            glyph: id,
            quarter_px: (size * 4.0).round() as u32,
            sub,
        };
        if let Some(hit) = self.slots.get(&key) {
            return *hit;
        }
        let slot = self.rasterize(font, id, key.quarter_px as f32 / 4.0, f32::from(sub) / 4.0);
        self.slots.insert(key, slot);
        slot
    }

    fn rasterize(&mut self, font: &FontData, id: u32, size: f32, offset: f32) -> Option<GlyphSlot> {
        let font_ref = FontRef::from_index(font.data.data(), font.index as usize)?;
        let glyph = u16::try_from(id).ok()?;
        let mut scaler = self
            .context
            .builder(font_ref)
            .size(size)
            .hint(false)
            .build();
        let image = Render::new(&[Source::Outline])
            .format(Format::Alpha)
            .offset(Vector::new(offset, 0.0))
            .render(&mut scaler, glyph)?;
        let (w, h) = (image.placement.width, image.placement.height);
        if w == 0 || h == 0 {
            return None;
        }
        let (page, x, y) = self.place(w, h)?;
        let target = &mut self.pages[page as usize];
        for row in 0..h {
            let src = (row * w) as usize;
            let dst = ((y + row) * target.width + x) as usize;
            target.pixels[dst..dst + w as usize]
                .copy_from_slice(&image.data[src..src + w as usize]);
        }
        target.generation += 1;
        Some(GlyphSlot {
            page,
            x,
            y,
            w,
            h,
            left: image.placement.left,
            top: image.placement.top,
        })
    }

    /// Room for a `w` × `h` mask: the first shelf on any page that fits, else a new shelf, else
    /// a new page.
    fn place(&mut self, w: u32, h: u32) -> Option<(u32, u32, u32)> {
        let side = self.page_size;
        if w + 2 * PAD > side || h + 2 * PAD > side {
            return None;
        }
        for (i, page) in self.pages.iter_mut().enumerate() {
            if let Some((x, y)) = page.fit(w, h) {
                return Some((i as u32, x, y));
            }
        }
        self.pages.push(AtlasPage {
            width: side,
            height: side,
            pixels: vec![0; (side * side) as usize],
            generation: 0,
            shelves: Vec::new(),
        });
        let last = self.pages.len() - 1;
        let (x, y) = self.pages[last].fit(w, h)?;
        Some((last as u32, x, y))
    }
}

impl AtlasPage {
    /// A spot for `w` × `h` (plus padding) on an existing or new shelf.
    fn fit(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let (pw, ph) = (w + 2 * PAD, h + 2 * PAD);
        for shelf in &mut self.shelves {
            if ph <= shelf.height && shelf.x + pw <= self.width {
                let at = (shelf.x + PAD, shelf.y + PAD);
                shelf.x += pw;
                return Some(at);
            }
        }
        let y = self.shelves.last().map_or(0, |s| s.y + s.height);
        if y + ph > self.height || pw > self.width {
            return None;
        }
        self.shelves.push(Shelf {
            y,
            height: ph,
            x: pw,
        });
        Some((PAD, y + PAD))
    }
}
