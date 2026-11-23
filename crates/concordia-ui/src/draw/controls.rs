//! Drawing interactive widgets' parts (2.7b.2) in the widget's text color: toggles, sliders,
//! dropdown faces, tab titles and focus rings with their node; scroll bars over their root's
//! nodes; open lists and tooltips above every root.

use concordia_api::{UiMask, UiPaint};

use super::{Base, Drawer, Px, unbounded};
use crate::controls::ToggleLook;
use crate::geom::Rect;
use crate::layout::{NodeLayout, UiLayout};
use crate::parts::{Parts, ROW_PAD, TAB_PAD_X, TAB_PAD_Y, TIP_PAD, TextBoxParts};
use crate::scene::UiElement;

/// The focus ring's color (linear RGBA).
pub const FOCUS_COLOR: [f32; 4] = [0.35, 0.65, 1.0, 1.0];
/// A switch's knob when on.
pub const KNOB_DARK: [f32; 4] = [0.08, 0.08, 0.1, 1.0];
/// An open list's background when its style has none.
pub const LIST_BG: [f32; 4] = [0.1, 0.11, 0.14, 0.98];
/// A tooltip's background.
pub const TIP_BG: [f32; 4] = [0.06, 0.06, 0.08, 0.95];
/// The focus ring's width and its distance outside the box.
const RING: f32 = 2.0;

fn alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], c[3] * a]
}

impl Drawer<'_> {
    /// A widget's own parts and its focus ring, drawn with its node.
    pub(super) fn controls(
        &mut self,
        el: &UiElement,
        node: &NodeLayout,
        parts: Option<&Parts>,
        base: &Base,
        px: &Px,
    ) {
        let c = node.style.color;
        match parts {
            Some(Parts::Toggle { knob }) => {
                let on = el.toggle.as_ref().is_some_and(|t| t.on);
                let look = el.toggle.as_ref().map(|t| t.look);
                match look {
                    Some(ToggleLook::Switch) => self.switch(*knob, on, c, base, px),
                    _ => self.checkbox(*knob, on, c, base, px),
                }
            }
            Some(Parts::Slider { track, grabber }) => {
                self.slider(*track, *grabber, el, c, base, px)
            }
            Some(Parts::Dropdown { face, arrow, .. }) => {
                if let Some(face) = face {
                    let content = node.content;
                    let y = content.y + (content.h - face.height) / 2.0;
                    self.glyphs(face, [content.x, y], base, px);
                }
                self.arrow(*arrow, c, base, px);
            }
            Some(Parts::TextBox(p)) => self.text_box(p, node, c, base, px),
            Some(Parts::Tabs { tabs }) => {
                let selected = el.tabs.as_ref().map_or(0, |t| t.selected as usize);
                let last = el.children.len().saturating_sub(1);
                for (i, tab) in tabs.iter().enumerate() {
                    let on = i == selected.min(last);
                    let r = tab.rect;
                    if on {
                        self.push(px.rect(r), UiPaint::Solid(alpha(c, 0.15)), base)
                            .radius = [px.len(4.0), px.len(4.0), 0.0, 0.0];
                        let line = Rect::new(r.x, r.y + r.h - 2.0, r.w, 2.0);
                        self.push(px.rect(line), UiPaint::Solid(c), base);
                    }
                    let mut faded = *base;
                    faded.opacity *= if on { 1.0 } else { 0.65 };
                    self.glyphs(&tab.text, [r.x + TAB_PAD_X, r.y + TAB_PAD_Y], &faded, px);
                }
            }
            _ => {}
        }
        if el.state.as_ref().is_some_and(|s| s.focused) {
            let r = node.rect;
            let ring = Rect::new(r.x - RING, r.y - RING, r.w + 2.0 * RING, r.h + 2.0 * RING);
            let quad = self.push(px.rect(ring), UiPaint::None, base);
            quad.border = px.len(RING);
            quad.border_color = FOCUS_COLOR;
            quad.radius = node.style.radius.map(|v| px.len(v + RING));
        }
    }

    /// The selection, the (masked) text or placeholder, and the caret, clipped to the content;
    /// then a spin box's arrows.
    fn text_box(&mut self, p: &TextBoxParts, node: &NodeLayout, c: [f32; 4], base: &Base, px: &Px) {
        let [x0, y0, x1, y1] = px.clip(node.content);
        let mut inner = *base;
        inner.clip = [
            base.clip[0].max(x0),
            base.clip[1].max(y0),
            base.clip[2].min(x1),
            base.clip[3].min(y1),
        ];
        for r in &p.selection {
            self.push(
                px.rect(*r),
                UiPaint::Solid(alpha(FOCUS_COLOR, 0.35)),
                &inner,
            );
        }
        let mut text = inner;
        if p.placeholder {
            text.opacity *= 0.5;
        }
        self.glyphs(&p.text, p.origin, &text, px);
        if let Some(caret) = p.caret {
            self.push(px.rect(caret), UiPaint::Solid(c), &inner);
        }
        if let Some((up, down)) = p.spin {
            for (half, pointing_up) in [(up, true), (down, false)] {
                self.push(px.rect(half), UiPaint::Solid(alpha(c, 0.08)), base);
                let r = Rect::new(
                    half.x + (half.w - 8.0) / 2.0,
                    half.y + (half.h - 4.0) / 2.0,
                    8.0,
                    4.0,
                );
                self.triangle(r, pointing_up, c, base, px);
            }
        }
    }

    fn checkbox(&mut self, knob: Rect, on: bool, c: [f32; 4], base: &Base, px: &Px) {
        let quad = self.push(px.rect(knob), UiPaint::None, base);
        quad.border = px.len(1.5);
        quad.border_color = c;
        quad.radius = [px.len(3.0); 4];
        if on {
            let check = Rect::new(knob.x + 4.0, knob.y + 4.0, knob.w - 8.0, knob.h - 8.0);
            self.push(px.rect(check), UiPaint::Solid(c), base).radius = [px.len(2.0); 4];
        }
    }

    fn switch(&mut self, track: Rect, on: bool, c: [f32; 4], base: &Base, px: &Px) {
        let fill = alpha(c, if on { 0.9 } else { 0.3 });
        self.push(px.rect(track), UiPaint::Solid(fill), base).radius = [px.len(track.h / 2.0); 4];
        let d = track.h - 4.0;
        let x = if on {
            track.x + track.w - 2.0 - d
        } else {
            track.x + 2.0
        };
        let knob = Rect::new(x, track.y + 2.0, d, d);
        let color = if on { KNOB_DARK } else { c };
        self.push(px.rect(knob), UiPaint::Solid(color), base).radius = [px.len(d / 2.0); 4];
    }

    fn slider(
        &mut self,
        track: Rect,
        grabber: Rect,
        el: &UiElement,
        c: [f32; 4],
        base: &Base,
        px: &Px,
    ) {
        let r = [px.len(track.w.min(track.h) / 2.0); 4];
        self.push(px.rect(track), UiPaint::Solid(alpha(c, 0.3)), base)
            .radius = r;
        let vertical = el
            .slider
            .is_some_and(|s| s.orientation == crate::widgets::Orientation::Vertical);
        let (gx, gy) = (grabber.x + grabber.w / 2.0, grabber.y + grabber.h / 2.0);
        let fill = if vertical {
            Rect::new(track.x, gy, track.w, track.y + track.h - gy)
        } else {
            Rect::new(track.x, track.y, gx - track.x, track.h)
        };
        self.push(px.rect(fill), UiPaint::Solid(c), base).radius = r;
        self.push(px.rect(grabber), UiPaint::Solid(c), base).radius = [px.len(grabber.w / 2.0); 4];
    }

    /// A downward triangle: the box masked to a wedge about its bottom middle.
    fn arrow(&mut self, r: Rect, c: [f32; 4], base: &Base, px: &Px) {
        self.triangle(r, false, c, base, px);
    }

    /// A triangle filling `r`, pointing up or down: the box masked to a wedge about the middle
    /// of the edge the point is on.
    fn triangle(&mut self, r: Rect, up: bool, c: [f32; 4], base: &Base, px: &Px) {
        let half = (r.w / 2.0).atan2(r.h);
        let (y, start) = if up {
            (r.y, std::f32::consts::PI)
        } else {
            (r.y + r.h, 0.0)
        };
        let center = px.point([r.x + r.w / 2.0, y]);
        self.push(px.rect(r), UiPaint::Solid(c), base).mask = UiMask::Angular {
            center,
            start,
            cw: half,
            ccw: half,
        };
    }

    /// The scroll bars of `root`'s nodes and its lists' selections, over their content.
    pub(super) fn bars(
        &mut self,
        layout: &UiLayout,
        elements: &std::collections::BTreeMap<concordia_core::id::EntityId, &UiElement>,
        root: concordia_core::id::EntityId,
        px: &Px,
    ) {
        for id in layout
            .order
            .iter()
            .filter(|id| layout.nodes[*id].root == root)
        {
            if let Some(el) = elements.get(id)
                && let Some(list) = &el.list
            {
                self.selections(layout, el, list, px);
            }
            let Some(Parts::Scroll {
                vertical,
                horizontal,
                ..
            }) = layout.parts.get(id)
            else {
                continue;
            };
            let node = &layout.nodes[id];
            let base = Base {
                clip: px.clip(node.clip),
                opacity: node.opacity,
            };
            let c = node.style.color;
            for bar in [vertical, horizontal].into_iter().flatten() {
                let r = [px.len(bar.track.w.min(bar.track.h) / 2.0); 4];
                self.push(px.rect(bar.track), UiPaint::Solid(alpha(c, 0.08)), &base)
                    .radius = r;
                self.push(px.rect(bar.thumb), UiPaint::Solid(alpha(c, 0.45)), &base)
                    .radius = r;
            }
        }
    }

    /// A list's selected items, highlighted over them.
    fn selections(
        &mut self,
        layout: &UiLayout,
        el: &UiElement,
        list: &crate::textbox::UiList,
        px: &Px,
    ) {
        for i in &list.selected {
            let Some(n) = el
                .children
                .get(*i as usize)
                .and_then(|item| layout.nodes.get(&item.entity))
            else {
                continue;
            };
            let base = Base {
                clip: px.clip(n.clip),
                opacity: n.opacity,
            };
            self.push(
                px.rect(n.rect),
                UiPaint::Solid(alpha(FOCUS_COLOR, 0.3)),
                &base,
            );
        }
    }

    /// Open lists, then tooltips, above every root.
    pub(super) fn top(&mut self, layout: &UiLayout, scale: f32) {
        let px_of = |root| {
            layout
                .roots
                .iter()
                .find(|r| r.entity == root)
                .map(|r| Px { map: r.map, scale })
        };
        let base = Base {
            clip: unbounded(),
            opacity: 1.0,
        };
        for id in &layout.order {
            let (
                Some(Parts::Dropdown {
                    list: Some(list), ..
                }),
                Some(node),
            ) = (layout.parts.get(id), layout.nodes.get(id))
            else {
                continue;
            };
            let Some(px) = px_of(node.root) else { continue };
            let c = node.style.color;
            let bg = match &node.style.background {
                Some(crate::style::Background::Solid(color)) => *color,
                _ => LIST_BG,
            };
            let quad = self.push(px.rect(list.rect), UiPaint::Solid(bg), &base);
            quad.radius = [px.len(4.0); 4];
            quad.border = px.len(1.0);
            quad.border_color = alpha(c, 0.3);
            for row in &list.rows {
                if row.index == list.highlight {
                    self.push(px.rect(row.rect), UiPaint::Solid(alpha(c, 0.18)), &base);
                }
                let at = [row.rect.x + 8.0, row.rect.y + ROW_PAD];
                self.glyphs(&row.text, at, &base, &px);
            }
        }
        for (id, tip) in &layout.tips {
            let Some(px) = layout.nodes.get(id).and_then(|n| px_of(n.root)) else {
                continue;
            };
            let quad = self.push(px.rect(tip.rect), UiPaint::Solid(TIP_BG), &base);
            quad.radius = [px.len(4.0); 4];
            quad.border = px.len(1.0);
            quad.border_color = [1.0, 1.0, 1.0, 0.25];
            self.glyphs(
                &tip.text,
                [tip.rect.x + TIP_PAD, tip.rect.y + TIP_PAD],
                &base,
                &px,
            );
        }
    }
}
