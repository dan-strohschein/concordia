//! Hit testing: the topmost node under a viewport point that takes the pointer, the widget it
//! belongs to and which part was hit. Open dropdown lists sit above everything; scroll bars sit
//! above their content.

use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::layout::UiLayout;
use crate::node::PointerFilter;
use crate::parts::{Bar, Parts};
use crate::scene::{UiElement, UiScene};
use crate::widgets::Orientation;

/// Which part of a widget was hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum HitPart {
    /// Its box.
    Body,
    /// An open dropdown's row (the option index).
    Row(u32),
    /// A tab title.
    Tab(u32),
    /// A scroll bar's thumb.
    Thumb(Orientation),
    /// A scroll bar's track outside the thumb.
    Track(Orientation),
    /// A spin box's up arrow.
    SpinUp,
    /// A spin box's down arrow.
    SpinDown,
    /// A list's item (its index among the list's children).
    Item(u32),
}

/// What is under a point.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// The root it belongs to.
    pub root: EntityId,
    /// The topmost node that takes the pointer.
    pub node: EntityId,
    /// The widget handling it: the node or its nearest interactive (or scrolling) ancestor.
    pub widget: Option<EntityId>,
    /// The part of the widget.
    pub part: HitPart,
    /// The point in the root's UI space.
    pub point: [f32; 2],
    /// Root to node.
    pub path: Vec<EntityId>,
}

/// A scene's elements by id, their parents and tree (preorder) order.
#[derive(Debug, Default)]
pub struct Index<'s> {
    /// Elements by id.
    pub elements: BTreeMap<EntityId, &'s UiElement>,
    /// Each element's parent.
    pub parents: BTreeMap<EntityId, EntityId>,
    /// Elements in tree order (roots by `z`, then id; children in layout order).
    pub preorder: Vec<EntityId>,
}

impl<'s> Index<'s> {
    /// Indexes `scene`.
    pub fn new(scene: &'s UiScene) -> Self {
        let mut index = Index::default();
        for tree in &scene.trees {
            index.add(&tree.element, None);
        }
        index
    }

    fn add(&mut self, el: &'s UiElement, parent: Option<EntityId>) {
        self.elements.insert(el.entity, el);
        if let Some(p) = parent {
            self.parents.insert(el.entity, p);
        }
        self.preorder.push(el.entity);
        for child in &el.children {
            self.add(child, Some(el.entity));
        }
    }

    /// `id` and its ancestors, nearest first.
    pub fn ancestry(&self, id: EntityId) -> Vec<EntityId> {
        let mut chain = vec![id];
        let mut at = id;
        while let Some(&p) = self.parents.get(&at) {
            chain.push(p);
            at = p;
        }
        chain
    }

    /// The nearest of `id` and its ancestors that `keep` accepts.
    pub fn nearest(&self, id: EntityId, keep: impl Fn(&UiElement) -> bool) -> Option<EntityId> {
        self.ancestry(id)
            .into_iter()
            .find(|a| self.elements.get(a).is_some_and(|e| keep(e)))
    }
}

/// Whether `el` takes the pointer: `Stop` always, `Ignore` never; `Auto` when it is interactive,
/// scrolls, has a tooltip, or draws a box or picture.
pub fn stops(el: &UiElement, layout: &UiLayout) -> bool {
    match el.node.pointer {
        PointerFilter::Stop => true,
        PointerFilter::Ignore => false,
        PointerFilter::Auto => {
            let style = layout.nodes.get(&el.entity).map(|n| &n.style);
            el.is_interactive()
                || el.scroll.is_some()
                || el.tooltip.is_some()
                || el.image.is_some()
                || el.nine.is_some()
                || el.progress.is_some()
                || style.is_some_and(|s| s.background.is_some() || s.border.is_some())
        }
    }
}

/// The topmost hit at viewport point `p`, if any.
pub fn hit_test(scene: &UiScene, layout: &UiLayout, p: [f32; 2]) -> Option<Hit> {
    let index = Index::new(scene);
    hit_with(&index, layout, p)
}

/// [`hit_test`] over a built index.
pub fn hit_with(index: &Index<'_>, layout: &UiLayout, p: [f32; 2]) -> Option<Hit> {
    let to_ui = |root: EntityId| {
        layout
            .roots
            .iter()
            .find(|r| r.entity == root)
            .map(|r| r.map.unapply(p))
    };
    // Open lists first, the last drawn on top.
    for &id in layout.order.iter().rev() {
        let (
            Some(Parts::Dropdown {
                list: Some(list), ..
            }),
            Some(node),
        ) = (layout.parts.get(&id), layout.nodes.get(&id))
        else {
            continue;
        };
        let q = to_ui(node.root)?;
        if let Some(row) = list.rows.iter().find(|r| r.rect.contains(q)) {
            return Some(hit(
                index,
                node.root,
                id,
                Some(id),
                HitPart::Row(row.index),
                q,
            ));
        }
        if list.rect.contains(q) {
            return Some(hit(index, node.root, id, Some(id), HitPart::Body, q));
        }
    }
    for &id in layout.order.iter().rev() {
        let (Some(node), Some(el)) = (layout.nodes.get(&id), index.elements.get(&id)) else {
            continue;
        };
        let q = to_ui(node.root)?;
        if !(node.rect.contains(q) && node.clip.contains(q)) {
            continue;
        }
        if let Some((scroll, part)) = bar_under(index, layout, id, q) {
            return Some(hit(index, node.root, scroll, Some(scroll), part, q));
        }
        if !stops(el, layout) {
            continue;
        }
        let widget = index.nearest(id, |e| {
            (e.is_interactive() || e.scroll.is_some()) && !e.disabled()
        });
        let part = widget
            .and_then(|w| {
                tab_under(layout, w, q)
                    .or_else(|| spin_under(layout, w, q))
                    .or_else(|| item_under(index, layout, w, q))
            })
            .unwrap_or(HitPart::Body);
        return Some(hit(index, node.root, id, widget, part, q));
    }
    None
}

fn hit(
    index: &Index<'_>,
    root: EntityId,
    node: EntityId,
    widget: Option<EntityId>,
    part: HitPart,
    point: [f32; 2],
) -> Hit {
    let mut path = index.ancestry(node);
    path.reverse();
    Hit {
        root,
        node,
        widget,
        part,
        point,
        path,
    }
}

/// A scroll bar (of `id` or an ancestor scroll) under `q`.
fn bar_under(
    index: &Index<'_>,
    layout: &UiLayout,
    id: EntityId,
    q: [f32; 2],
) -> Option<(EntityId, HitPart)> {
    index.ancestry(id).into_iter().find_map(|a| {
        let Some(Parts::Scroll {
            vertical,
            horizontal,
            ..
        }) = layout.parts.get(&a)
        else {
            return None;
        };
        let on = |bar: &Option<Bar>, axis: Orientation| {
            bar.as_ref()
                .filter(|b| b.track.contains(q))
                .map(|b| match b.thumb.contains(q) {
                    true => HitPart::Thumb(axis),
                    false => HitPart::Track(axis),
                })
        };
        on(vertical, Orientation::Vertical)
            .or_else(|| on(horizontal, Orientation::Horizontal))
            .map(|part| (a, part))
    })
}

/// The tab title of tabs widget `w` under `q`.
fn tab_under(layout: &UiLayout, w: EntityId, q: [f32; 2]) -> Option<HitPart> {
    let Some(Parts::Tabs { tabs }) = layout.parts.get(&w) else {
        return None;
    };
    tabs.iter()
        .position(|t| t.rect.contains(q))
        .map(|i| HitPart::Tab(i as u32))
}

/// The spin arrow of `w` under `q`.
fn spin_under(layout: &UiLayout, w: EntityId, q: [f32; 2]) -> Option<HitPart> {
    let Some(Parts::TextBox(p)) = layout.parts.get(&w) else {
        return None;
    };
    let (up, down) = p.spin?;
    if up.contains(q) {
        Some(HitPart::SpinUp)
    } else if down.contains(q) {
        Some(HitPart::SpinDown)
    } else {
        None
    }
}

/// The item of list `w` under `q`.
fn item_under(index: &Index<'_>, layout: &UiLayout, w: EntityId, q: [f32; 2]) -> Option<HitPart> {
    let el = index.elements.get(&w)?;
    el.list.as_ref()?;
    el.children
        .iter()
        .position(|c| {
            layout
                .nodes
                .get(&c.entity)
                .is_some_and(|n| n.rect.contains(q))
        })
        .map(|i| HitPart::Item(i as u32))
}
