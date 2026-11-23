//! Layout: each root's canvas is mapped to the viewport; taffy lays out every in-flow subtree
//! (flex, grid, stack) and anchored nodes start their own island inside their parent's rect.
//! Styles are resolved on the way down; text is shaped at its final width. All derived data,
//! never hashed (ADR-0060).

use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use taffy::compute_leaf_layout;
use taffy::prelude::{
    AlignItems, AvailableSpace, Dimension, Display, FlexDirection, FlexWrap, GridTemplateComponent,
    JustifyContent, LengthPercentage, LengthPercentageAuto, Line, NodeId, Size, Style, TaffyTree,
    auto, fr, length, line, percent,
};

use crate::anchor::anchored_rect;
use crate::assets::UiAssets;
use crate::error::UiError;
use crate::fonts::UiFonts;
use crate::geom::Rect;
use crate::node::{Align, Direction, Justify, Layout, Length, Position, Track, UiNode};
use crate::root::{CanvasMap, canvas_map};
use crate::scene::{UiElement, UiScene};
use crate::style::{ResolvedStyle, UiThemed, WidgetState};
use crate::text::{TextBlock, shape};
use crate::theme::{ThemeClass, resolve};
use crate::widgets::UiText;

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
}

/// No clip: a rect far larger than any canvas.
pub const UNBOUNDED: Rect = Rect::new(-1.0e7, -1.0e7, 2.0e7, 2.0e7);

/// Lays out `scene` for a `viewport` in logical pixels.
pub fn compute_layout(
    scene: &UiScene,
    fonts: &mut UiFonts,
    assets: &mut dyn UiAssets,
    viewport: [f32; 2],
) -> UiLayout {
    let mut out = UiLayout {
        problems: scene.problems.clone(),
        ..UiLayout::default()
    };
    for tree in &scene.trees {
        let map = canvas_map(&tree.root, viewport);
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
            styles: BTreeMap::new(),
            rects: BTreeMap::new(),
        };
        pass.resolve(&tree.element, None);
        pass.island(&tree.element, Rect::new(0.0, 0.0, map.size[0], map.size[1]));
        pass.finish(&tree.element, UNBOUNDED, 1.0);
    }
    out
}

/// What a taffy leaf measures.
#[derive(Debug, Clone, Copy)]
enum Leaf {
    Text(EntityId),
    Image([f32; 2]),
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
    styles: BTreeMap<EntityId, ResolvedStyle>,
    rects: BTreeMap<EntityId, Rect>,
}

impl Pass<'_> {
    /// Resolves styles top-down (themes, then inheritance).
    fn resolve(&mut self, el: &UiElement, parent: Option<&ResolvedStyle>) {
        let class = el.themed.as_ref().and_then(|t| self.class(el.entity, t));
        let style = resolve(
            el.style.as_ref(),
            class.as_ref(),
            WidgetState::Normal,
            parent,
        );
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
        let (fonts, assets, styles) = (&mut *self.fonts, &mut *self.assets, &self.styles);
        let elements = collect_texts(&built);
        let measured =
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
            });
        if measured.is_err() {
            return;
        }
        self.read(&tree, &built, rect.x, rect.y, true);
    }

    /// Records each built node's rect (origin plus its taffy location) and lays out anchored
    /// children inside it.
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

    fn build<'e>(
        &mut self,
        tree: &mut TaffyTree<Leaf>,
        el: &'e UiElement,
        parent: Option<&Layout>,
        fixed: Option<[f32; 2]>,
    ) -> Result<Built<'e>, taffy::TaffyError> {
        let style = taffy_style(&el.node, parent, fixed);
        let mut kids = Vec::new();
        let mut anchored = Vec::new();
        for child in &el.children {
            match child.node.position {
                Position::Anchored(_) => anchored.push(child),
                Position::InFlow => {
                    kids.push(self.build(tree, child, Some(&el.node.layout), None)?)
                }
            }
        }
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

    fn leaf(&mut self, el: &UiElement) -> Option<Leaf> {
        if el.text.is_some() {
            return Some(Leaf::Text(el.entity));
        }
        let image = el.image.as_ref()?;
        let data = self.assets.image(&image.image)?;
        let size = match image.region {
            Some([_, _, w, h]) => [w as f32, h as f32],
            None => data.size(),
        };
        Some(Leaf::Image(size))
    }

    /// Shapes text at its final size and records every node top-down, clips and opacity
    /// accumulating, siblings drawn by (`z`, `order`, id).
    fn finish(&mut self, el: &UiElement, clip: Rect, opacity: f32) {
        let Some(rect) = self.rects.get(&el.entity).copied() else {
            return;
        };
        let style = self.styles.get(&el.entity).cloned().unwrap_or_default();
        let opacity = opacity * style.opacity;
        let content = rect.inset(&el.node.padding);
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
        let inner = if el.node.clip {
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
        Some(Leaf::Image(size)) => size,
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
