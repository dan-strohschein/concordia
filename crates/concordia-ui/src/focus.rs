//! Keyboard focus: Tab order and arrow moves (WAI-ARIA's keyboard patterns, Godot's focus
//! neighbours, for semantics).

use concordia_core::id::EntityId;

use crate::hit::Index;
use crate::layout::UiLayout;
use crate::scene::UiElement;

/// An arrow direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Up (−y).
    Up,
    /// Down (+y).
    Down,
    /// Left (−x).
    Left,
    /// Right (+x).
    Right,
}

/// Whether `el` can take focus: an enabled button, toggle, slider, dropdown or tabs.
pub fn focusable(el: &UiElement) -> bool {
    el.is_interactive() && !el.disabled()
}

/// Focusable widgets that were laid out, in Tab order: those with a `UiFocus.order` first
/// (ascending), then tree order.
pub fn tab_order(index: &Index<'_>, layout: &UiLayout) -> Vec<EntityId> {
    let mut ids: Vec<(usize, EntityId)> = index
        .preorder
        .iter()
        .enumerate()
        .filter(|(_, id)| layout.nodes.contains_key(id))
        .filter(|(_, id)| index.elements.get(id).is_some_and(|e| focusable(e)))
        .map(|(i, id)| (i, *id))
        .collect();
    let order = |id: &EntityId| index.elements[id].focus.and_then(|f| f.order);
    ids.sort_by_key(|(i, id)| (order(id).is_none(), order(id).unwrap_or(0), *i));
    ids.into_iter().map(|(_, id)| id).collect()
}

/// Where Tab (or Shift-Tab when `back`) goes from `from`: its explicit neighbour, else the
/// next (previous) in Tab order, wrapping; from nothing, the first (last).
pub fn step(
    index: &Index<'_>,
    layout: &UiLayout,
    from: Option<EntityId>,
    back: bool,
) -> Option<EntityId> {
    let order = tab_order(index, layout);
    let explicit = from
        .and_then(|f| index.elements.get(&f)?.focus)
        .and_then(|f| if back { f.prev } else { f.next })
        .filter(|t| order.contains(t));
    if explicit.is_some() {
        return explicit;
    }
    let n = order.len();
    if n == 0 {
        return None;
    }
    let at = from.and_then(|f| order.iter().position(|o| *o == f));
    let i = match (at, back) {
        (None, false) => 0,
        (None, true) => n - 1,
        (Some(i), false) => (i + 1) % n,
        (Some(i), true) => (i + n - 1) % n,
    };
    Some(order[i])
}

/// Where an arrow goes from `from`: its explicit neighbour, else the focusable widget whose
/// center lies that way with the least distance along plus twice the distance across (viewport
/// space; ties go to Tab order).
pub fn arrow(index: &Index<'_>, layout: &UiLayout, from: EntityId, dir: Dir) -> Option<EntityId> {
    let order = tab_order(index, layout);
    let explicit = index
        .elements
        .get(&from)
        .and_then(|e| e.focus)
        .and_then(|f| match dir {
            Dir::Up => f.up,
            Dir::Down => f.down,
            Dir::Left => f.left,
            Dir::Right => f.right,
        });
    if let Some(t) = explicit.filter(|t| order.contains(t)) {
        return Some(t);
    }
    let center = |id: &EntityId| center(layout, *id);
    let origin = center(&from)?;
    let mut best: Option<(f32, EntityId)> = None;
    for id in order.iter().filter(|id| **id != from) {
        let Some(c) = center(id) else { continue };
        let (dx, dy) = (c[0] - origin[0], c[1] - origin[1]);
        let (along, across) = match dir {
            Dir::Right => (dx, dy.abs()),
            Dir::Left => (-dx, dy.abs()),
            Dir::Down => (dy, dx.abs()),
            Dir::Up => (-dy, dx.abs()),
        };
        if along <= 0.0 {
            continue;
        }
        let score = along + 2.0 * across;
        if best.is_none_or(|(s, _)| score < s) {
            best = Some((score, *id));
        }
    }
    best.map(|(_, id)| id)
}

/// A node's center in viewport space.
fn center(layout: &UiLayout, id: EntityId) -> Option<[f32; 2]> {
    let node = layout.nodes.get(&id)?;
    let map = layout.roots.iter().find(|r| r.entity == node.root)?.map;
    let r = node.rect;
    Some(map.apply([r.x + r.w / 2.0, r.y + r.h / 2.0]))
}
