//! Layout: each root's canvas is mapped to the viewport; taffy lays out every in-flow subtree
//! (flex, grid, stack) and anchored nodes start their own island inside their parent's rect.
//! Styles are resolved on the way down; text is shaped at its final width. All derived data,
//! never hashed (ADR-0060).

use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Vec3};
use taffy::compute_leaf_layout;
use taffy::prelude::{
    AlignItems, AvailableSpace, Dimension, Display, FlexDirection, FlexWrap, GridTemplateComponent,
    JustifyContent, LengthPercentage, LengthPercentageAuto, Line, NodeId, Size, Style, TaffyTree,
    auto, fr, length, line, percent,
};

use crate::anchor::anchored_rect;
use crate::assets::UiAssets;
use crate::controls::ToggleLook;
use crate::error::UiError;
use crate::fonts::UiFonts;
use crate::geom::Rect;
use crate::node::{Align, Direction, Justify, Layout, Length, Position, Track, UiNode};
use crate::parts::{
    CARET_MARGIN, CARET_W, Label, List, Parts, ROW_PAD, Row, SLIDER_SIZE, SPIN_W, TAB_PAD_X,
    TAB_PAD_Y, TEXT_BOX_SIZE, TextBoxParts, Tip, clamp_offset, dropdown_arrow, first_row,
    list_rect, scroll_bars, slider_parts, tip_rect, toggle_knob,
};
use crate::root::{CanvasMap, RootSpace, canvas_map};
use crate::scene::{UiElement, UiScene, UiTree};
use crate::style::{ResolvedStyle, UiThemed, WidgetState};
use crate::text::{TextBlock, caret_x, shape};
use crate::theme::{ThemeClass, resolve};
use crate::widgets::{Orientation, Overflow, UiText, Wrap};

/// One node's results.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeLayout {
    /// Its root.
    pub root: EntityId,
    /// Its box in UI space.
    pub rect: Rect,
    /// Its box less padding.
    pub content: Rect,
    /// What its drawing is clipped to (UI space; unbounded when nothing clips).
    pub clip: Rect,
    /// Its resolved style.
    pub style: ResolvedStyle,
    /// Its opacity times every ancestor's.
    pub opacity: f32,
}

/// One root's results.
#[derive(Debug, Clone, PartialEq)]
pub struct RootLayout {
    /// The root entity.
    pub entity: EntityId,
    /// How its UI space maps to the viewport.
    pub map: CanvasMap,
    /// Its draw order among roots.
    pub z: i32,
}

/// Everything layout derives from a scene.
#[derive(Debug, Clone, Default)]
pub struct UiLayout {
    /// Roots, back to front.
    pub roots: Vec<RootLayout>,
    /// Every laid-out node.
    pub nodes: BTreeMap<EntityId, NodeLayout>,
    /// Shaped labels.
    pub texts: BTreeMap<EntityId, TextBlock>,
    /// Every node, back to front.
    pub order: Vec<EntityId>,
    /// The scene's problems plus layout's.
    pub problems: Vec<(EntityId, UiError)>,
    /// Interactive widgets' parts.
    pub parts: BTreeMap<EntityId, Parts>,
    /// Tooltips showing.
    pub tips: BTreeMap<EntityId, Tip>,
}

/// No clip: a rect far larger than any canvas.
pub const UNBOUNDED: Rect = Rect::new(-1.0e7, -1.0e7, 2.0e7, 2.0e7);

/// Lays out `scene` for a `viewport` in logical pixels (screen roots only).
pub fn compute_layout(
    scene: &UiScene,
    fonts: &mut UiFonts,
    assets: &mut dyn UiAssets,
    viewport: [f32; 2],
) -> UiLayout {
    compute_layout_seen(scene, fonts, assets, viewport, None)
}

/// Lays out `scene` for a `viewport` seen through `clip_from_world`: world roots first, far to
/// near (hidden behind the camera, past its far plane or off screen), then screen roots.
pub fn compute_layout_seen(
    scene: &UiScene,
    fonts: &mut UiFonts,
    assets: &mut dyn UiAssets,
    viewport: [f32; 2],
    clip_from_world: Option<Mat4>,
) -> UiLayout {
    let mut out = UiLayout {
        problems: scene.problems.clone(),
        ..UiLayout::default()
    };
    for (tree, map) in placed_roots(scene, viewport, clip_from_world) {
        let entity = tree.element.entity;
        out.roots.push(RootLayout {
            entity,
            map,
            z: tree.root.z,
        });
        let mut pass = Pass {
            fonts: &mut *fonts,
            assets: &mut *assets,
            out: &mut out,
            root: entity,
            space: map.size,
            styles: BTreeMap::new(),
            rects: BTreeMap::new(),
        };
        pass.resolve(&tree.element, None);
        pass.island(&tree.element, Rect::new(0.0, 0.0, map.size[0], map.size[1]));
        pass.finish(&tree.element, UNBOUNDED, 1.0);
    }
    out
}

/// The roots to lay out with their maps: world roots seen by the camera, far to near, then
/// screen roots in order.
fn placed_roots(
    scene: &UiScene,
    viewport: [f32; 2],
    clip_from_world: Option<Mat4>,
) -> Vec<(&UiTree, CanvasMap)> {
    let mut world: Vec<(f32, &UiTree, CanvasMap)> = scene
        .trees
        .iter()
        .filter_map(|tree| {
            let RootSpace::World { offset, pixel_size } = tree.root.space else {
                return None;
            };
            let p = tree.position? + Vec3::from(offset);
            let (center, depth) = project(clip_from_world?, p, viewport)?;
            let size = tree.root.canvas;
            let map = CanvasMap {
                scale: [pixel_size, pixel_size],
                offset: [
                    center[0] - size[0] * pixel_size / 2.0,
                    center[1] - size[1] * pixel_size / 2.0,
                ],
                size,
            };
            let on_screen = map.offset[0] < viewport[0]
                && map.offset[1] < viewport[1]
                && map.offset[0] + size[0] * pixel_size > 0.0
                && map.offset[1] + size[1] * pixel_size > 0.0;
            on_screen.then_some((depth, tree, map))
        })
        .collect();
    world.sort_by(|a, b| a.0.total_cmp(&b.0));
    let screen = scene
        .trees
        .iter()
        .filter(|t| t.root.space == RootSpace::Screen)
        .map(|t| (t, canvas_map(&t.root, viewport)));
    world
        .into_iter()
        .map(|(_, t, m)| (t, m))
        .chain(screen)
        .collect()
}

/// A world point's viewport position (logical pixels) and depth (0 far .. 1 near, reversed Z),
/// or `None` behind the eye or outside the depth range.
fn project(clip_from_world: Mat4, p: Vec3, viewport: [f32; 2]) -> Option<([f32; 2], f32)> {
    let clip = clip_from_world * p.extend(1.0);
    if clip.w <= 1e-6 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if !(0.0..=1.0).contains(&ndc.z) {
        return None;
    }
    Some((
        [
            (ndc.x + 1.0) / 2.0 * viewport[0],
            (1.0 - ndc.y) / 2.0 * viewport[1],
        ],
        ndc.z,
    ))
}

/// What a taffy leaf measures.
#[derive(Debug, Clone, Copy)]
enum Leaf {
    Text(EntityId),
    Fixed([f32; 2]),
}

/// A subtree built into taffy: its node, in-flow children and anchored children.
struct Built<'e> {
    id: NodeId,
    element: &'e UiElement,
    kids: Vec<Built<'e>>,
    anchored: Vec<&'e UiElement>,
}

/// One root's layout in progress.
struct Pass<'a> {
    fonts: &'a mut UiFonts,
    assets: &'a mut dyn UiAssets,
    out: &'a mut UiLayout,
    root: EntityId,
    space: [f32; 2],
    styles: BTreeMap<EntityId, ResolvedStyle>,
    rects: BTreeMap<EntityId, Rect>,
}

impl Pass<'_> {
    /// Resolves styles top-down (themes, then inheritance).
    fn resolve(&mut self, el: &UiElement, parent: Option<&ResolvedStyle>) {
        let class = el.themed.as_ref().and_then(|t| self.class(el.entity, t));
        let style = resolve(el.style.as_ref(), class.as_ref(), widget_state(el), parent);
        for child in &el.children {
            self.resolve(child, Some(&style));
        }
        self.styles.insert(el.entity, style);
    }

    fn class(&mut self, entity: EntityId, themed: &UiThemed) -> Option<ThemeClass> {
        let theme = self.assets.theme(&themed.theme)?;
        let class = theme.classes.get(&themed.class).cloned();
        if class.is_none() {
            self.out.problems.push((
                entity,
                UiError::field(
                    "UiThemed",
                    "class",
                    format!("{} has no class {:?}", themed.theme, themed.class),
                ),
            ));
        }
        class
    }

    /// Lays out `el`'s in-flow subtree in `rect`, then each anchored node as its own island.
    fn island(&mut self, el: &UiElement, rect: Rect) {
        let mut tree: TaffyTree<Leaf> = TaffyTree::new();
        tree.disable_rounding();
        let Ok(built) = self.build(&mut tree, el, None, Some([rect.w, rect.h])) else {
            return;
        };
        let space = Size {
            width: AvailableSpace::Definite(rect.w),
            height: AvailableSpace::Definite(rect.h),
        };
        if self.compute(&mut tree, &built, space).is_ok() {
            self.read(&tree, &built, rect.x, rect.y, true);
        }
    }

    /// Runs taffy over a built subtree, measuring its leaves.
    fn compute(
        &mut self,
        tree: &mut TaffyTree<Leaf>,
        built: &Built<'_>,
        space: Size<AvailableSpace>,
    ) -> Result<(), taffy::TaffyError> {
        let (fonts, assets, styles) = (&mut *self.fonts, &mut *self.assets, &self.styles);
        let elements = collect_texts(built);
        tree.compute_layout_with_measure(built.id, space, |input, _, leaf, style| {
            compute_leaf_layout(
                input,
                style,
                |_, _| 0.0,
                |known, available| {
                    measure(
                        leaf.copied(),
                        known,
                        available,
                        fonts,
                        assets,
                        styles,
                        &elements,
                    )
                },
            )
        })
    }

    /// Records each built node's rect (origin plus its taffy location), lays out a scroll's
    /// content and anchored children inside it.
    fn read(&mut self, tree: &TaffyTree<Leaf>, built: &Built<'_>, x: f32, y: f32, root: bool) {
        let Ok(l) = tree.layout(built.id) else {
            return;
        };
        let (lx, ly) = if root {
            (0.0, 0.0)
        } else {
            (l.location.x, l.location.y)
        };
        let rect = Rect::new(x + lx, y + ly, l.size.width, l.size.height);
        self.rects.insert(built.element.entity, rect);
        for kid in &built.kids {
            self.read(tree, kid, rect.x, rect.y, false);
        }
        if built.element.scroll.is_some() {
            self.scroll_island(built.element, rect);
        }
        for child in &built.anchored {
            let Position::Anchored(a) = &child.node.position else {
                continue;
            };
            let min = [
                child.node.min_width.px().unwrap_or(0.0),
                child.node.min_height.px().unwrap_or(0.0),
            ];
            self.island(child, anchored_rect(rect, a, min));
        }
    }

    /// Lays out a scroll's in-flow children unbounded along its scrolling axes, at least as big
    /// as the scroll, shifted by the clamped offset.
    fn scroll_island(&mut self, el: &UiElement, rect: Rect) {
        let Some(scroll) = &el.scroll else {
            return;
        };
        let mut tree: TaffyTree<Leaf> = TaffyTree::new();
        tree.disable_rounding();
        let mut style = taffy_style(&el.node, None, None);
        let extras = self.extras(el);
        extras.apply(&mut style, &el.node);
        style.margin = taffy::Rect::zero();
        let axis = |scrolls: bool, len: f32| match scrolls {
            true => (Dimension::auto(), AvailableSpace::MaxContent),
            false => (Dimension::length(len), AvailableSpace::Definite(len)),
        };
        let (w, aw) = axis(scroll.scrolls(0), rect.w);
        let (h, ah) = axis(scroll.scrolls(1), rect.h);
        style.size = Size {
            width: w,
            height: h,
        };
        style.min_size = Size {
            width: LengthPercentageAuto::length(rect.w),
            height: LengthPercentageAuto::length(rect.h),
        };
        style.max_size = Size::auto();
        let Ok(kids) = self.build_kids(&mut tree, el, false) else {
            return;
        };
        let ids: Vec<NodeId> = kids.0.iter().map(|k| k.id).collect();
        let Ok(id) = tree.new_with_children(style, &ids) else {
            return;
        };
        let built = Built {
            id,
            element: el,
            kids: kids.0,
            anchored: Vec::new(),
        };
        let space = Size {
            width: aw,
            height: ah,
        };
        if self.compute(&mut tree, &built, space).is_err() {
            return;
        }
        let Ok(l) = tree.layout(built.id) else {
            return;
        };
        let content = [l.size.width, l.size.height];
        let offset = clamp_offset(scroll, [rect.w, rect.h], content);
        for kid in &built.kids {
            self.read(&tree, kid, rect.x - offset[0], rect.y - offset[1], false);
        }
        let (vertical, horizontal) = scroll_bars(scroll, rect, content, offset);
        self.out.parts.insert(
            el.entity,
            Parts::Scroll {
                offset,
                content,
                vertical,
                horizontal,
            },
        );
    }

    fn build<'e>(
        &mut self,
        tree: &mut TaffyTree<Leaf>,
        el: &'e UiElement,
        parent: Option<&Layout>,
        fixed: Option<[f32; 2]>,
    ) -> Result<Built<'e>, taffy::TaffyError> {
        let mut style = taffy_style(&el.node, parent, fixed);
        self.extras(el).apply(&mut style, &el.node);
        let (kids, anchored) = self.build_kids(tree, el, el.scroll.is_some())?;
        let id = match (kids.is_empty(), self.leaf(el)) {
            (true, Some(leaf)) => tree.new_leaf_with_context(style, leaf)?,
            _ => {
                let ids: Vec<NodeId> = kids.iter().map(|k| k.id).collect();
                tree.new_with_children(style, &ids)?
            }
        };
        Ok(Built {
            id,
            element: el,
            kids,
            anchored,
        })
    }

    /// `el`'s children to lay out: in-flow ones built (none when `skip_flow`, for a scroll whose
    /// content is its own island) and anchored ones set aside. Tabs keep only the selected page.
    #[allow(clippy::type_complexity)]
    fn build_kids<'e>(
        &mut self,
        tree: &mut TaffyTree<Leaf>,
        el: &'e UiElement,
        skip_flow: bool,
    ) -> Result<(Vec<Built<'e>>, Vec<&'e UiElement>), taffy::TaffyError> {
        let page = el.tabs.as_ref().map(|t| {
            let last = el.children.len().saturating_sub(1);
            (t.selected as usize).min(last)
        });
        let mut kids = Vec::new();
        let mut anchored = Vec::new();
        for (i, child) in el.children.iter().enumerate() {
            if page.is_some_and(|p| p != i) {
                continue;
            }
            match child.node.position {
                Position::Anchored(_) => anchored.push(child),
                Position::InFlow if skip_flow => {}
                Position::InFlow => {
                    kids.push(self.build(tree, child, Some(&el.node.layout), None)?)
                }
            }
        }
        Ok((kids, anchored))
    }

    fn leaf(&mut self, el: &UiElement) -> Option<Leaf> {
        if let Some(slider) = &el.slider {
            let [w, h] = SLIDER_SIZE;
            return Some(Leaf::Fixed(match slider.orientation {
                Orientation::Horizontal => [w, h],
                Orientation::Vertical => [h, w],
            }));
        }
        if let Some(b) = &el.text_box {
            let style = self.styles.get(&el.entity).cloned().unwrap_or_default();
            let line = self.line_height(&style);
            let lines = if b.multiline { 4.0 } else { 1.0 };
            let width = if b.multiline { 240.0 } else { TEXT_BOX_SIZE[0] };
            return Some(Leaf::Fixed([width, line * lines]));
        }
        if let Some(dropdown) = &el.dropdown {
            let style = self.styles.get(&el.entity).cloned().unwrap_or_default();
            let widest = dropdown
                .options
                .iter()
                .map(|o| self.label(o, &style).width)
                .fold(0.0, f32::max);
            let gap = style.font_size * 0.5;
            let arrow = style.font_size * 0.5;
            return Some(Leaf::Fixed([
                widest + gap + arrow,
                self.line_height(&style),
            ]));
        }
        if el.text.is_some() {
            return Some(Leaf::Text(el.entity));
        }
        let image = el.image.as_ref()?;
        let data = self.assets.image(&image.image)?;
        let size = match image.region {
            Some([_, _, w, h]) => [w as f32, h as f32],
            None => data.size(),
        };
        Some(Leaf::Fixed(size))
    }

    /// Space a widget adds inside its box: a toggle's box left of the label, the tab row on top.
    fn extras(&mut self, el: &UiElement) -> Extras {
        let style = self.styles.get(&el.entity).cloned().unwrap_or_default();
        let mut extras = Extras::default();
        if let Some(toggle) = &el.toggle {
            let size = style.font_size;
            if let Some(knob) = toggle_knob(toggle.look, Rect::new(0.0, 0.0, 0.0, size), size) {
                extras.left = knob.w + toggle.label_gap;
                extras.min_height = size;
            }
        }
        if el.tabs.is_some() {
            extras.top = self.line_height(&style) + 2.0 * TAB_PAD_Y;
        }
        if el.spin.is_some() && el.text_box.is_some() {
            extras.right = SPIN_W;
        }
        if let Some(text) = &el.text {
            self.text_floor(el, text, &style, &mut extras);
        }
        extras
    }

    /// A label's box never squeezes its text (Godot's `Label`): its minimum height is the text
    /// wrapped at the box's fixed width, and unwrapped text sets its minimum width too.
    /// `Clip` and `Ellipsis` opt out.
    fn text_floor(
        &mut self,
        el: &UiElement,
        text: &UiText,
        style: &ResolvedStyle,
        extras: &mut Extras,
    ) {
        if text.overflow != Overflow::Visible {
            return;
        }
        let width = match el.node.width {
            Length::Px(w) => {
                Some((w - el.node.padding.horizontal() - extras.left - extras.right).max(0.0))
            }
            _ => None,
        };
        let block = shape(self.fonts, self.assets, text, style, width, None);
        extras.min_height = extras.min_height.max(block.height);
        if text.wrap == Wrap::None {
            extras.min_width = block.width;
        }
    }

    /// `text` shaped on one line in `style`.
    fn label(&mut self, text: &str, style: &ResolvedStyle) -> TextBlock {
        shape(
            self.fonts,
            self.assets,
            &UiText::plain(text),
            style,
            None,
            None,
        )
    }

    /// One line's height in `style`.
    fn line_height(&mut self, style: &ResolvedStyle) -> f32 {
        self.label("M", style).height
    }

    /// Shapes text at its final size and records every node top-down, clips and opacity
    /// accumulating, siblings drawn by (`z`, `order`, id).
    fn finish(&mut self, el: &UiElement, clip: Rect, opacity: f32) {
        let Some(rect) = self.rects.get(&el.entity).copied() else {
            return;
        };
        let style = self.styles.get(&el.entity).cloned().unwrap_or_default();
        let opacity = opacity * style.opacity;
        let extras = self.extras(el);
        let plain = rect.inset(&el.node.padding);
        let content = Rect::new(
            plain.x + extras.left,
            plain.y + extras.top,
            (plain.w - extras.left - extras.right).max(0.0),
            (plain.h - extras.top).max(0.0),
        );
        if let Some(text) = &el.text {
            let block = shape(
                self.fonts,
                self.assets,
                text,
                &style,
                Some(content.w),
                Some(content.h),
            );
            self.out.texts.insert(el.entity, block);
        }
        self.parts(el, rect, plain, &style);
        if let Some(b) = &el.text_box {
            let parts = self.text_box(el, b, content, plain, &style);
            self.out.parts.insert(el.entity, Parts::TextBox(parts));
        }
        self.tip(el, &style);
        self.out.nodes.insert(
            el.entity,
            NodeLayout {
                root: self.root,
                rect,
                content,
                clip,
                style,
                opacity,
            },
        );
        self.out.order.push(el.entity);
        let inner = if el.node.clip || el.scroll.is_some() {
            clip.intersect(&rect)
        } else {
            clip
        };
        let mut kids: Vec<&UiElement> = el.children.iter().collect();
        kids.sort_by_key(|c| (c.node.z, c.node.order, c.entity));
        for child in kids {
            self.finish(child, inner, opacity);
        }
    }

    /// Records a toggle's, slider's, dropdown's or tabs' parts (scrolls record theirs when laid
    /// out). `plain` is the content box before a widget's extras.
    fn parts(&mut self, el: &UiElement, rect: Rect, plain: Rect, style: &ResolvedStyle) {
        let parts = if let Some(toggle) = &el.toggle {
            toggle_knob(toggle.look, plain, style.font_size).map(|knob| Parts::Toggle { knob })
        } else if let Some(slider) = &el.slider {
            let (track, grabber) = slider_parts(slider, plain);
            Some(Parts::Slider { track, grabber })
        } else if let Some(dropdown) = &el.dropdown {
            Some(self.dropdown(el, dropdown, rect, plain, style))
        } else if let Some(tabs) = &el.tabs {
            let h = self.line_height(style) + 2.0 * TAB_PAD_Y;
            let mut x = plain.x;
            let tabs = tabs
                .titles
                .iter()
                .map(|title| {
                    let text = self.label(title, style);
                    let rect = Rect::new(x, plain.y, text.width + 2.0 * TAB_PAD_X, h);
                    x += rect.w;
                    Label { rect, text }
                })
                .collect();
            Some(Parts::Tabs { tabs })
        } else {
            None
        };
        if let Some(parts) = parts {
            self.out.parts.insert(el.entity, parts);
        }
    }

    fn dropdown(
        &mut self,
        el: &UiElement,
        dropdown: &crate::controls::UiDropdown,
        rect: Rect,
        plain: Rect,
        style: &ResolvedStyle,
    ) -> Parts {
        let face = dropdown
            .selected
            .and_then(|i| dropdown.options.get(i as usize))
            .map(|o| self.label(o, style));
        let arrow = dropdown_arrow(plain, style.font_size);
        let count = dropdown.options.len() as u32;
        let list = (dropdown.open && count > 0).then(|| {
            let row_h = self.line_height(style) + 2.0 * ROW_PAD;
            let visible = count.min(dropdown.max_visible);
            let highlight = el
                .state
                .as_ref()
                .and_then(|s| s.item)
                .or(dropdown.selected)
                .unwrap_or(0);
            let highlight = highlight.min(count - 1);
            let first = first_row(highlight, visible, count);
            let list = list_rect(rect, visible, row_h, self.space[1]);
            let rows = (first..first + visible)
                .enumerate()
                .map(|(i, index)| Row {
                    index,
                    rect: Rect::new(list.x, list.y + i as f32 * row_h, list.w, row_h),
                    text: self.label(&dropdown.options[index as usize], style),
                })
                .collect();
            List {
                rect: list,
                rows,
                highlight,
            }
        });
        Parts::Dropdown { face, arrow, list }
    }

    /// A text box's drawn text, caret, selection and scroll (and a spin box's arrows).
    fn text_box(
        &mut self,
        el: &UiElement,
        b: &crate::textbox::UiTextBox,
        content: Rect,
        plain: Rect,
        style: &ResolvedStyle,
    ) -> TextBoxParts {
        let placeholder = b.text.is_empty() && !b.placeholder.is_empty();
        let shown = if placeholder {
            b.placeholder.clone()
        } else {
            b.shown()
        };
        let wrap = if b.multiline { b.wrap } else { Wrap::None };
        let text = UiText {
            wrap,
            ..UiText::plain(shown)
        };
        let width = b.multiline.then_some(content.w);
        let block = shape(self.fonts, self.assets, &text, style, width, None);
        let dy = if b.multiline {
            0.0
        } else {
            (content.h - block.height) / 2.0
        };
        let focused = el.state.as_ref().is_some_and(|s| s.focused);
        let caret_at = |at: usize| caret_x(&block, b.shown_offset(at));
        let scroll = match (b.multiline, caret_at(b.caret as usize)) {
            (false, Some((x, _))) if !placeholder => (x + CARET_MARGIN - content.w).max(0.0),
            _ => 0.0,
        };
        let origin = [content.x - scroll, content.y + dy];
        let caret = (focused && !placeholder || focused && b.text.is_empty())
            .then(|| {
                let (x, line) = if placeholder {
                    (0.0, 0)
                } else {
                    caret_at(b.caret as usize)?
                };
                let l = block.lines.get(line)?;
                let h = if placeholder {
                    block.height.max(l.height)
                } else {
                    l.height
                };
                Some(Rect::new(origin[0] + x, origin[1] + l.top, CARET_W, h))
            })
            .flatten();
        let sel = b.selection();
        let (s0, s1) = (b.shown_offset(sel.start), b.shown_offset(sel.end));
        let selection = if placeholder || sel.is_empty() {
            Vec::new()
        } else {
            block
                .lines
                .iter()
                .filter(|l| s0 < l.end.max(l.start + 1) && s1 > l.start)
                .map(|l| {
                    let x_of = |at: usize| {
                        l.stops
                            .iter()
                            .rev()
                            .find(|(b, _)| *b <= at)
                            .map_or(l.x, |(_, x)| *x)
                    };
                    let (x0, x1) = (x_of(s0.max(l.start)), x_of(s1.min(l.end)));
                    Rect::new(origin[0] + x0, origin[1] + l.top, x1 - x0, l.height)
                })
                .collect()
        };
        let spin = el.spin.is_some().then(|| {
            let x = plain.x + plain.w - SPIN_W;
            let half = plain.h / 2.0;
            (
                Rect::new(x, plain.y, SPIN_W, half),
                Rect::new(x, plain.y + half, SPIN_W, half),
            )
        });
        TextBoxParts {
            text: block,
            origin,
            placeholder,
            caret,
            selection,
            scroll,
            spin,
        }
    }

    /// Records a tooltip that has waited out its delay.
    fn tip(&mut self, el: &UiElement, style: &ResolvedStyle) {
        let (Some(tooltip), Some(state)) = (&el.tooltip, &el.state) else {
            return;
        };
        let Some(at) = state
            .tip
            .filter(|_| state.hover_ticks >= tooltip.delay_ticks)
        else {
            return;
        };
        let text = shape(
            self.fonts,
            self.assets,
            &UiText::plain(tooltip.text.clone()),
            style,
            Some(TIP_MAX_WIDTH),
            None,
        );
        let rect = tip_rect(at, [text.width, text.height], self.space);
        self.out.tips.insert(el.entity, Tip { rect, text });
    }
}

/// The widest a tooltip's text wraps at.
const TIP_MAX_WIDTH: f32 = 320.0;

/// What a widget adds inside its box.
#[derive(Debug, Clone, Copy, Default)]
struct Extras {
    left: f32,
    top: f32,
    right: f32,
    /// The least content height (0: none).
    min_height: f32,
    /// The least content width (0: none).
    min_width: f32,
}

impl Extras {
    fn apply(&self, s: &mut Style, node: &UiNode) {
        s.padding.left = LengthPercentage::length(node.padding.left + self.left);
        s.padding.top = LengthPercentage::length(node.padding.top + self.top);
        s.padding.right = LengthPercentage::length(node.padding.right + self.right);
        if self.min_height > 0.0 && node.min_height == Length::Auto {
            s.min_size.height =
                LengthPercentageAuto::length(self.min_height + node.padding.vertical() + self.top);
        }
        if self.min_width > 0.0 && node.min_width == Length::Auto {
            s.min_size.width = LengthPercentageAuto::length(
                self.min_width + node.padding.horizontal() + self.left + self.right,
            );
        }
    }
}

/// The theme state a widget draws in: disabled, pressed (or an on `Button` toggle), hovered,
/// focused, else normal.
pub fn widget_state(el: &UiElement) -> WidgetState {
    let s = el.state.clone().unwrap_or_default();
    let on_button = el
        .toggle
        .as_ref()
        .is_some_and(|t| t.on && t.look == ToggleLook::Button);
    if s.disabled {
        WidgetState::Disabled
    } else if s.pressed || on_button {
        WidgetState::Pressed
    } else if s.hovered {
        WidgetState::Hover
    } else if s.focused {
        WidgetState::Focused
    } else {
        WidgetState::Normal
    }
}

/// Every text element in a built subtree, by entity.
fn collect_texts<'e>(built: &Built<'e>) -> BTreeMap<EntityId, &'e UiText> {
    let mut out = BTreeMap::new();
    let mut stack = vec![built];
    while let Some(b) = stack.pop() {
        if let Some(text) = &b.element.text {
            out.insert(b.element.entity, text);
        }
        stack.extend(b.kids.iter());
    }
    out
}

/// A leaf's intrinsic size: text shaped at the known or available width; images at their size.
fn measure(
    leaf: Option<Leaf>,
    known: Size<Option<f32>>,
    available: Size<AvailableSpace>,
    fonts: &mut UiFonts,
    assets: &mut dyn UiAssets,
    styles: &BTreeMap<EntityId, ResolvedStyle>,
    texts: &BTreeMap<EntityId, &UiText>,
) -> Size<f32> {
    let natural = match leaf {
        None => [0.0, 0.0],
        Some(Leaf::Fixed(size)) => size,
        Some(Leaf::Text(entity)) => {
            let (Some(text), Some(style)) = (texts.get(&entity), styles.get(&entity)) else {
                return Size::ZERO;
            };
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(w) => Some(w),
                AvailableSpace::MinContent => Some(0.0),
                AvailableSpace::MaxContent => None,
            });
            let block = shape(fonts, assets, text, style, width, None);
            [block.width, block.height]
        }
    };
    Size {
        width: known.width.unwrap_or(natural[0]),
        height: known.height.unwrap_or(natural[1]),
    }
}

fn dimension(length: Length) -> Dimension {
    match length {
        Length::Auto => Dimension::auto(),
        Length::Px(v) => Dimension::length(v),
        Length::Percent(p) => Dimension::percent(p / 100.0),
    }
}

fn bound(length: Length) -> LengthPercentageAuto {
    match length {
        Length::Auto => LengthPercentageAuto::auto(),
        Length::Px(v) => LengthPercentageAuto::length(v),
        Length::Percent(p) => LengthPercentageAuto::percent(p / 100.0),
    }
}

fn track(t: &Track) -> GridTemplateComponent<String> {
    GridTemplateComponent::Single(match *t {
        Track::Px(v) => length(v),
        Track::Fr(v) => fr(v),
        Track::Percent(p) => percent(p / 100.0),
        Track::Auto => auto(),
    })
}

fn justify(j: Justify, flex: bool) -> JustifyContent {
    match (j, flex) {
        (Justify::Start, true) => JustifyContent::FLEX_START,
        (Justify::End, true) => JustifyContent::FLEX_END,
        (Justify::Start, false) => JustifyContent::START,
        (Justify::End, false) => JustifyContent::END,
        (Justify::Center, _) => JustifyContent::CENTER,
        (Justify::SpaceBetween, _) => JustifyContent::SPACE_BETWEEN,
        (Justify::SpaceAround, _) => JustifyContent::SPACE_AROUND,
        (Justify::SpaceEvenly, _) => JustifyContent::SPACE_EVENLY,
    }
}

fn align(a: Align, flex: bool) -> AlignItems {
    match (a, flex) {
        (Align::Stretch, _) => AlignItems::STRETCH,
        (Align::Start, true) => AlignItems::FLEX_START,
        (Align::End, true) => AlignItems::FLEX_END,
        (Align::Start, false) => AlignItems::START,
        (Align::End, false) => AlignItems::END,
        (Align::Center, _) => AlignItems::CENTER,
    }
}

/// A node's taffy style; `fixed` sizes an island's root, `parent` places stack children.
fn taffy_style(node: &UiNode, parent: Option<&Layout>, fixed: Option<[f32; 2]>) -> Style {
    let e = |v: f32| LengthPercentageAuto::length(v);
    let p = |v: f32| LengthPercentage::length(v);
    let mut s = Style {
        size: Size {
            width: dimension(node.width),
            height: dimension(node.height),
        },
        min_size: Size {
            width: bound(node.min_width),
            height: bound(node.min_height),
        },
        max_size: Size {
            width: bound(node.max_width),
            height: bound(node.max_height),
        },
        margin: taffy::Rect {
            left: e(node.margin.left),
            right: e(node.margin.right),
            top: e(node.margin.top),
            bottom: e(node.margin.bottom),
        },
        padding: taffy::Rect {
            left: p(node.padding.left),
            right: p(node.padding.right),
            top: p(node.padding.top),
            bottom: p(node.padding.bottom),
        },
        gap: Size {
            width: p(node.gap[0]),
            height: p(node.gap[1]),
        },
        flex_grow: node.grow,
        flex_shrink: node.shrink,
        ..Style::default()
    };
    if let Some([w, h]) = fixed {
        s.size = Size {
            width: Dimension::length(w),
            height: Dimension::length(h),
        };
        s.margin = taffy::Rect::zero();
    }
    match &node.layout {
        Layout::Flex(f) => {
            s.display = Display::Flex;
            s.flex_direction = match f.direction {
                Direction::Row => FlexDirection::Row,
                Direction::Column => FlexDirection::Column,
                Direction::RowReverse => FlexDirection::RowReverse,
                Direction::ColumnReverse => FlexDirection::ColumnReverse,
            };
            s.flex_wrap = if f.wrap {
                FlexWrap::Wrap
            } else {
                FlexWrap::NoWrap
            };
            s.justify_content = justify(f.justify, true);
            s.align_items = align(f.align_items, true);
            s.align_content = justify(f.align_content, true);
        }
        Layout::Grid(g) => {
            s.display = Display::Grid;
            s.grid_template_columns = g.columns.iter().map(track).collect();
            s.grid_template_rows = g.rows.iter().map(track).collect();
            s.justify_items = align(g.justify_items, false);
            s.align_items = align(g.align_items, false);
        }
        Layout::Stack => {
            s.display = Display::Grid;
            s.grid_template_columns = vec![GridTemplateComponent::Single(fr(1.0))];
            s.grid_template_rows = vec![GridTemplateComponent::Single(fr(1.0))];
        }
    }
    if matches!(parent, Some(Layout::Stack)) {
        s.grid_row = Line {
            start: line(1),
            end: line(2),
        };
        s.grid_column = Line {
            start: line(1),
            end: line(2),
        };
    }
    s
}
