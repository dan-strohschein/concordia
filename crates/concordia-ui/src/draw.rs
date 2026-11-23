//! The draw list: every laid-out node's background, border, nine-patch, image, progress,
//! separator and glyphs, mapped from UI space to target pixels (the root's canvas map, then the
//! frame's scale), back to front.

use std::collections::BTreeMap;
use std::sync::Arc;

use concordia_api::{UiDrawList, UiFrame, UiMask, UiPaint, UiQuad, UiTexture, UiTextureFormat};
use concordia_assets::AssetRef;
use concordia_core::id::EntityId;

use crate::assets::{ImageData, UiAssets};
use crate::atlas::GlyphAtlas;
use crate::geom::Rect;
use crate::layout::{NodeLayout, UNBOUNDED, UiLayout};
use crate::paint::{
    background_paint, image_quads, nine_patch_quads, progress_fill, separator_rect,
};
use crate::root::CanvasMap;
use crate::scene::{UiElement, UiScene};
use crate::style::ResolvedStyle;
use crate::text::TextBlock;
use crate::widgets::{Overflow, UiImage, UiNinePatch, UiProgress, UiText, VAlign};

mod controls;

pub use controls::{FOCUS_COLOR, KNOB_DARK, LIST_BG, TIP_BG};

/// The `ui_bounds` outline color (linear RGBA).
pub const BOUNDS_COLOR: [f32; 4] = [0.1, 0.9, 1.0, 1.0];
/// Atlas pages' texture keys have this bit set; images' never do.
const ATLAS_KEY: u64 = 1 << 63;

/// Builds the draw list for `layout` (computed for `frame.logical()`).
pub fn draw_list(
    scene: &UiScene,
    layout: &UiLayout,
    atlas: &mut GlyphAtlas,
    assets: &mut dyn UiAssets,
    frame: &UiFrame,
) -> UiDrawList {
    let mut elements = BTreeMap::new();
    for tree in &scene.trees {
        index(&tree.element, &mut elements);
    }
    let mut d = Drawer {
        list: UiDrawList::default(),
        images: BTreeMap::new(),
        pages: BTreeMap::new(),
        atlas,
        assets,
    };
    for root in &layout.roots {
        let px = Px {
            map: root.map,
            scale: frame.scale,
        };
        for id in layout
            .order
            .iter()
            .filter(|id| layout.nodes[*id].root == root.entity)
        {
            if let Some(el) = elements.get(id) {
                d.node(el, &layout.nodes[id], layout.texts.get(id), &px);
                let base = Base {
                    clip: px.clip(layout.nodes[id].clip),
                    opacity: layout.nodes[id].opacity,
                };
                d.controls(el, &layout.nodes[id], layout.parts.get(id), &base, &px);
            }
        }
        d.bars(layout, root.entity, &px);
    }
    d.top(layout, frame.scale);
    if frame.bounds {
        d.bounds(layout, frame.scale);
    }
    d.finish()
}

fn index<'e>(el: &'e UiElement, out: &mut BTreeMap<EntityId, &'e UiElement>) {
    out.insert(el.entity, el);
    for child in &el.children {
        index(child, out);
    }
}

/// UI space → target pixels.
#[derive(Debug, Clone, Copy)]
struct Px {
    map: CanvasMap,
    scale: f32,
}

impl Px {
    fn point(&self, p: [f32; 2]) -> [f32; 2] {
        let v = self.map.apply(p);
        [v[0] * self.scale, v[1] * self.scale]
    }

    fn rect(&self, r: Rect) -> [f32; 4] {
        let [x, y] = self.point([r.x, r.y]);
        [
            x,
            y,
            r.w * self.map.scale[0] * self.scale,
            r.h * self.map.scale[1] * self.scale,
        ]
    }

    fn len(&self, v: f32) -> f32 {
        v * self.map.scale[0].min(self.map.scale[1]) * self.scale
    }

    fn clip(&self, r: Rect) -> [f32; 4] {
        if r == UNBOUNDED {
            return unbounded();
        }
        let [x, y, w, h] = self.rect(r);
        [x, y, x + w, y + h]
    }
}

fn unbounded() -> [f32; 4] {
    UiQuad::new([0.0; 4], UiPaint::None).clip
}

fn as_rect([x, y, w, h]: [f32; 4]) -> Rect {
    Rect::new(x, y, w, h)
}

struct Drawer<'a> {
    list: UiDrawList,
    images: BTreeMap<String, Option<(u32, Arc<ImageData>)>>,
    pages: BTreeMap<u32, u32>,
    atlas: &'a mut GlyphAtlas,
    assets: &'a mut dyn UiAssets,
}

impl Drawer<'_> {
    fn node(&mut self, el: &UiElement, node: &NodeLayout, text: Option<&TextBlock>, px: &Px) {
        let base = Base {
            clip: px.clip(node.clip),
            opacity: node.opacity,
        };
        let rect = px.rect(node.rect);
        self.background(&node.style, rect, &base, px);
        if let Some(nine) = &el.nine {
            self.nine_patch(nine, node.rect, &base, px);
        }
        if let Some(image) = &el.image {
            self.image(image, node.rect, &base, px);
        }
        if let Some(progress) = &el.progress {
            self.progress(progress, node, &base, px);
        }
        if let Some(sep) = &el.separator {
            self.push(
                px.rect(separator_rect(sep, node.rect)),
                UiPaint::Solid(sep.color),
                &base,
            );
        }
        if let (Some(t), Some(block)) = (&el.text, text) {
            self.text(t, block, node, &base, px);
        }
    }

    fn push(&mut self, rect: [f32; 4], paint: UiPaint, base: &Base) -> &mut UiQuad {
        let mut quad = UiQuad::new(rect, paint);
        quad.clip = base.clip;
        quad.opacity = base.opacity;
        self.list.quads.push(quad);
        self.list.quads.last_mut().unwrap_or_else(|| unreachable!())
    }

    fn background(&mut self, style: &ResolvedStyle, rect: [f32; 4], base: &Base, px: &Px) {
        if style.background.is_none() && style.border.is_none() {
            return;
        }
        let paint = style
            .background
            .as_ref()
            .map_or(UiPaint::None, |b| background_paint(b, as_rect(rect)));
        let quad = self.push(rect, paint, base);
        quad.radius = style.radius.map(|r| px.len(r));
        if let Some(border) = style.border {
            quad.border = px.len(border.width);
            quad.border_color = border.color;
        }
    }

    /// The texture index and data for an image, uploaded once per list.
    fn texture(&mut self, image: &AssetRef) -> Option<(u32, Arc<ImageData>)> {
        let key = image.to_string();
        if let Some(hit) = self.images.get(&key) {
            return hit.clone();
        }
        let found = self.assets.image(image).map(|data| {
            let index = self.list.textures.len() as u32;
            let hash = blake3::hash(key.as_bytes());
            let mut id = [0u8; 8];
            id.copy_from_slice(&hash.as_bytes()[..8]);
            self.list.textures.push(UiTexture {
                key: u64::from_le_bytes(id) & !ATLAS_KEY,
                generation: 0,
                width: data.width,
                height: data.height,
                format: if data.srgb {
                    UiTextureFormat::Rgba8Srgb
                } else {
                    UiTextureFormat::Rgba8Linear
                },
                pixels: data.pixels.clone(),
            });
            (index, data)
        });
        self.images.insert(key, found.clone());
        found
    }

    fn textured(
        &mut self,
        pieces: Vec<(Rect, [f32; 4])>,
        texture: u32,
        tint: [f32; 4],
        base: &Base,
        px: &Px,
    ) {
        for (r, uv) in pieces {
            self.push(px.rect(r), UiPaint::Texture { texture, uv, tint }, base);
        }
    }

    fn image(&mut self, image: &UiImage, rect: Rect, base: &Base, px: &Px) {
        let Some((texture, data)) = self.texture(&image.image) else {
            return;
        };
        let Some((size, uv)) = region(&data, image.region) else {
            return;
        };
        let mut pieces = image_quads(image.mode, rect, size, uv, [image.flip_h, image.flip_v]);
        if image.region.is_some() {
            pieces = pieces
                .into_iter()
                .map(|(r, uv)| (r, half_texel_inset(uv, data.size())))
                .collect();
        }
        self.textured(pieces, texture, image.tint, base, px);
    }

    fn nine_patch(&mut self, nine: &UiNinePatch, rect: Rect, base: &Base, px: &Px) {
        let Some((texture, data)) = self.texture(&nine.image) else {
            return;
        };
        let [x, y, w, h] = nine
            .region
            .unwrap_or([0, 0, data.width, data.height])
            .map(|v| v as f32);
        let [l, t, r, b] = nine.margins.map(|v| v as f32);
        if l + r > w || t + b > h || x + w > data.width as f32 || y + h > data.height as f32 {
            return;
        }
        let pieces = nine_patch_quads(
            rect,
            [x, y, w, h],
            data.size(),
            [l, t, r, b],
            [nine.axis_h, nine.axis_v],
            nine.draw_center,
        );
        let inset = pieces
            .into_iter()
            .map(|(r, uv)| (r, half_texel_inset(uv, data.size())))
            .collect();
        self.textured(inset, texture, nine.tint, base, px);
    }

    fn progress(&mut self, progress: &UiProgress, node: &NodeLayout, base: &Base, px: &Px) {
        let rect = px.rect(node.rect);
        let radius = node.style.radius.map(|r| px.len(r));
        let fill = progress_fill(progress, node.rect);
        let mask = |m: UiMask| match m {
            UiMask::Angular {
                center,
                start,
                cw,
                ccw,
            } => UiMask::Angular {
                center: px.point(center),
                start,
                cw,
                ccw,
            },
            UiMask::None => UiMask::None,
        };
        if let Some(fill_texture) = &progress.progress {
            let full = [0.0, 0.0, 1.0, 1.0];
            if let Some((t, _)) = progress.under.as_ref().and_then(|u| self.texture(u)) {
                self.push(
                    rect,
                    UiPaint::Texture {
                        texture: t,
                        uv: full,
                        tint: [1.0; 4],
                    },
                    base,
                );
            }
            if let (Some((t, _)), Some((r, uv, m))) = (self.texture(fill_texture), fill) {
                self.push(
                    px.rect(r),
                    UiPaint::Texture {
                        texture: t,
                        uv,
                        tint: progress.tint,
                    },
                    base,
                )
                .mask = mask(m);
            }
            if let Some((t, _)) = progress.over.as_ref().and_then(|o| self.texture(o)) {
                self.push(
                    rect,
                    UiPaint::Texture {
                        texture: t,
                        uv: full,
                        tint: [1.0; 4],
                    },
                    base,
                );
            }
            return;
        }
        if let Some(track) = &progress.track {
            self.push(rect, background_paint(track, as_rect(rect)), base)
                .radius = radius;
        }
        if let (Some(bar), Some((r, _, m))) = (&progress.bar, fill) {
            let quad = self.push(px.rect(r), background_paint(bar, as_rect(rect)), base);
            quad.radius = radius;
            quad.mask = mask(m);
        }
    }

    fn text(&mut self, text: &UiText, block: &TextBlock, node: &NodeLayout, base: &Base, px: &Px) {
        let content = node.content;
        let dy = match text.valign {
            VAlign::Top => 0.0,
            VAlign::Center => (content.h - block.height) / 2.0,
            VAlign::Bottom => content.h - block.height,
        };
        let mut base = *base;
        if text.overflow == Overflow::Clip {
            let [x0, y0, x1, y1] = px.clip(content);
            let c = base.clip;
            base.clip = [c[0].max(x0), c[1].max(y0), c[2].min(x1), c[3].min(y1)];
        }
        self.glyphs(block, [content.x, content.y + dy], &base, px);
        for (r, run) in &block.underlines {
            let color = block.colors.get(*run as usize).copied().unwrap_or([1.0; 4]);
            let line = Rect::new(content.x + r[0], content.y + dy + r[1], r[2], r[3]);
            self.push(px.rect(line), UiPaint::Solid(color), &base);
        }
    }

    /// A text block's glyphs with their top left at `origin` (UI space).
    fn glyphs(&mut self, block: &TextBlock, origin: [f32; 2], base: &Base, px: &Px) {
        let size_scale = px.map.scale[1] * px.scale;
        for g in &block.glyphs {
            let [x, y] = px.point([origin[0] + g.x, origin[1] + g.y]);
            let Some(font) = block.fonts.get(g.font as usize) else {
                continue;
            };
            let Some(slot) = self
                .atlas
                .glyph(font, g.id, g.size * size_scale, x - x.floor())
            else {
                continue;
            };
            let texture = self.page(slot.page);
            let (pw, ph) = self
                .atlas
                .pages()
                .get(slot.page as usize)
                .map_or((1.0, 1.0), |p| (p.width as f32, p.height as f32));
            let rect = [
                x.floor() + slot.left as f32,
                y.round() - slot.top as f32,
                slot.w as f32,
                slot.h as f32,
            ];
            let uv = [
                slot.x as f32 / pw,
                slot.y as f32 / ph,
                (slot.x + slot.w) as f32 / pw,
                (slot.y + slot.h) as f32 / ph,
            ];
            let color = block
                .colors
                .get(g.run as usize)
                .copied()
                .unwrap_or([1.0; 4]);
            self.push(rect, UiPaint::Glyph { texture, uv, color }, base);
        }
    }

    /// The texture index of atlas page `page` (its pixels are filled in at the end).
    fn page(&mut self, page: u32) -> u32 {
        if let Some(i) = self.pages.get(&page) {
            return *i;
        }
        let index = self.list.textures.len() as u32;
        self.list.textures.push(UiTexture {
            key: ATLAS_KEY | u64::from(page),
            generation: 0,
            width: 0,
            height: 0,
            format: UiTextureFormat::R8,
            pixels: Arc::from(Vec::new()),
        });
        self.pages.insert(page, index);
        index
    }

    fn bounds(&mut self, layout: &UiLayout, scale: f32) {
        let base = Base {
            clip: unbounded(),
            opacity: 1.0,
        };
        for root in &layout.roots {
            let px = Px {
                map: root.map,
                scale,
            };
            for id in layout
                .order
                .iter()
                .filter(|id| layout.nodes[*id].root == root.entity)
            {
                let quad = self.push(px.rect(layout.nodes[id].rect), UiPaint::None, &base);
                quad.border = 1.0;
                quad.border_color = BOUNDS_COLOR;
            }
        }
    }

    fn finish(mut self) -> UiDrawList {
        for (page, index) in &self.pages {
            if let (Some(p), Some(t)) = (
                self.atlas.pages().get(*page as usize),
                self.list.textures.get_mut(*index as usize),
            ) {
                t.generation = p.generation;
                t.width = p.width;
                t.height = p.height;
                t.pixels = Arc::from(p.pixels.as_slice());
            }
        }
        self.list
    }
}

/// What every quad of a node shares.
#[derive(Debug, Clone, Copy)]
struct Base {
    clip: [f32; 4],
    opacity: f32,
}

/// `uv` pulled in by half a texel on each side, so linear filtering never samples the
/// neighbouring slice (a mirrored rect stays mirrored; a rect under a texel wide is kept).
pub fn half_texel_inset(uv: [f32; 4], texture: [f32; 2]) -> [f32; 4] {
    let inset = |a: f32, b: f32, size: f32| {
        let half = 0.5 / size.max(1.0);
        match (b - a).abs() > 2.0 * half {
            true if b > a => (a + half, b - half),
            true => (a - half, b + half),
            false => (a, b),
        }
    };
    let (u0, u1) = inset(uv[0], uv[2], texture[0]);
    let (v0, v1) = inset(uv[1], uv[3], texture[1]);
    [u0, v0, u1, v1]
}

/// An image region's pixel size and UV rect (`None` when it lies outside the image).
fn region(data: &ImageData, region: Option<[u32; 4]>) -> Option<([f32; 2], [f32; 4])> {
    let [x, y, w, h] = region.unwrap_or([0, 0, data.width, data.height]);
    if x + w > data.width || y + h > data.height || w == 0 || h == 0 {
        return None;
    }
    let (tw, th) = (data.width as f32, data.height as f32);
    Some((
        [w as f32, h as f32],
        [
            x as f32 / tw,
            y as f32 / th,
            (x + w) as f32 / tw,
            (y + h) as f32 / th,
        ],
    ))
}
