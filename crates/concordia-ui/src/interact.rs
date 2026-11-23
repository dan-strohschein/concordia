//! One tick of interaction (`ui.interact`, 2.7b.2): the `ui_*` actions move the pointer, press,
//! drag, scroll and focus widgets; widget values and `UiState` are written back to the world,
//! signals and sound cues are raised, and the pointer is reported consumed when the UI took it.
//! Runs inside the simulation from recorded actions, so replays reproduce it exactly.

use std::collections::BTreeMap;

use concordia_assets::AssetRef;
use concordia_audio::PlayCue;
use concordia_core::id::EntityId;
use concordia_input::{ActionStates, ActionValue};
use concordia_terra::{Component, World};

use crate::controls::{
    ActionOn, Grab, UiDropdown, UiFocusChanged, UiPressed, UiScroll, UiSelected, UiSlider,
    UiSounds, UiState, UiTabs, UiToggle, UiToggled, UiValueChanged,
};
use crate::focus::{Dir, arrow, focusable, step};
use crate::hit::{Hit, HitPart, Index, hit_with};
use crate::layout::UiLayout;
use crate::parts::{Parts, clamp_offset, offset_for_thumb, slider_value_at};
use crate::scene::{UiElement, UiScene};
use crate::widgets::Orientation;

/// The pointer position action (Axis2d, logical pixels).
pub const POINTER: &str = "ui_pointer";
/// The click action (Button).
pub const CLICK: &str = "ui_click";
/// The wheel action (Axis2d, lines; positive y scrolls up).
pub const SCROLL: &str = "ui_scroll";
/// The viewport action (Axis2d, logical pixels).
pub const VIEWPORT: &str = "ui_viewport";
/// Focus and widget keys, in [`Keys`] order.
pub const KEYS: [&str; 10] = [
    "ui_up",
    "ui_down",
    "ui_left",
    "ui_right",
    "ui_accept",
    "ui_cancel",
    "ui_next",
    "ui_prev",
    "ui_page_up",
    "ui_page_down",
];

/// A button action's edges this tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Press {
    /// Held.
    pub pressed: bool,
    /// Went down this tick.
    pub just_pressed: bool,
    /// Went up this tick.
    pub just_released: bool,
}

/// Keys that went down this tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Keys {
    /// ↑.
    pub up: bool,
    /// ↓.
    pub down: bool,
    /// ←.
    pub left: bool,
    /// →.
    pub right: bool,
    /// Enter or Space.
    pub accept: bool,
    /// Escape.
    pub cancel: bool,
    /// Tab.
    pub next: bool,
    /// Shift-Tab.
    pub prev: bool,
    /// Page Up.
    pub page_up: bool,
    /// Page Down.
    pub page_down: bool,
}

/// What the UI reads from the tick's actions.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct UiInput {
    /// The pointer in viewport logical pixels (`None` without a `ui_pointer` action).
    pub pointer: Option<[f32; 2]>,
    /// The click.
    pub click: Press,
    /// Wheel lines this tick.
    pub scroll: [f32; 2],
    /// The viewport (`None` until one is known).
    pub viewport: Option<[f32; 2]>,
    /// Keys.
    pub keys: Keys,
}

impl UiInput {
    /// Reads the `ui_*` actions (missing ones read as idle).
    pub fn read(states: &ActionStates) -> Self {
        let axis = |name: &str| match states.get(name).map(|s| s.value) {
            Some(ActionValue::Axis2d(v)) => Some([v.x, v.y]),
            _ => None,
        };
        let press = |name: &str| {
            states.get(name).map_or(Press::default(), |s| Press {
                pressed: s.pressed,
                just_pressed: s.just_pressed,
                just_released: s.just_released,
            })
        };
        let down: Vec<bool> = KEYS.iter().map(|k| press(k).just_pressed).collect();
        let prev = down[7];
        Self {
            pointer: axis(POINTER),
            click: press(CLICK),
            scroll: axis(SCROLL).unwrap_or_default(),
            viewport: axis(VIEWPORT).filter(|v| v[0] > 0.0 && v[1] > 0.0),
            keys: Keys {
                up: down[0],
                down: down[1],
                left: down[2],
                right: down[3],
                accept: down[4],
                cancel: down[5],
                // Shift-Tab also holds Tab: back wins.
                next: down[6] && !prev,
                prev,
                page_up: down[8],
                page_down: down[9],
            },
        }
    }
}

/// The viewport to lay out for: the input's, else the first root's canvas.
pub fn sim_viewport(scene: &UiScene, input: &UiInput) -> [f32; 2] {
    input
        .viewport
        .or_else(|| scene.trees.first().map(|t| t.root.canvas))
        .unwrap_or([1.0, 1.0])
}

/// A raised signal, applied after the tick's changes.
enum Signal {
    Pressed,
    Toggled(bool),
    Value(f32),
    Selected(u32),
    Focus(bool),
    Cue(AssetRef),
}

/// The tick's working copy of widget values and states.
struct Tick<'s> {
    index: Index<'s>,
    layout: &'s UiLayout,
    states: BTreeMap<EntityId, UiState>,
    toggles: BTreeMap<EntityId, UiToggle>,
    sliders: BTreeMap<EntityId, UiSlider>,
    dropdowns: BTreeMap<EntityId, UiDropdown>,
    tabs: BTreeMap<EntityId, UiTabs>,
    scrolls: BTreeMap<EntityId, UiScroll>,
    signals: Vec<(EntityId, Signal)>,
    viewport_point: Option<[f32; 2]>,
}

/// Runs one tick of interaction over `world`'s UI; returns whether the pointer was consumed.
pub fn interact(world: &mut World, scene: &UiScene, layout: &UiLayout, input: &UiInput) -> bool {
    let mut tick = Tick::new(scene, layout, input.pointer);
    let hit = input.pointer.and_then(|p| hit_with(&tick.index, layout, p));
    tick.hover(hit.as_ref());
    let mut consumed = hit.is_some();
    if input.click.just_pressed {
        consumed |= tick.press(hit.as_ref());
    }
    if input.click.pressed || input.click.just_pressed {
        tick.drag();
    }
    if input.click.just_released {
        tick.release(hit.as_ref());
    }
    if input.scroll != [0.0, 0.0]
        && let Some(hit) = &hit
    {
        tick.wheel(hit, input.scroll);
    }
    tick.keys(&input.keys);
    consumed |= tick.states.values().any(|s| s.pressed);
    tick.apply(world);
    consumed
}

impl<'s> Tick<'s> {
    fn new(scene: &'s UiScene, layout: &'s UiLayout, pointer: Option<[f32; 2]>) -> Self {
        let index = Index::new(scene);
        let mut tick = Tick {
            index,
            layout,
            states: BTreeMap::new(),
            toggles: BTreeMap::new(),
            sliders: BTreeMap::new(),
            dropdowns: BTreeMap::new(),
            tabs: BTreeMap::new(),
            scrolls: BTreeMap::new(),
            signals: Vec::new(),
            viewport_point: pointer,
        };
        let elements: Vec<&UiElement> = tick.index.elements.values().copied().collect();
        for el in elements {
            let id = el.entity;
            if el.is_interactive() || el.scroll.is_some() || el.tooltip.is_some() {
                tick.states.insert(id, el.state.clone().unwrap_or_default());
            }
            copy(&mut tick.toggles, id, &el.toggle);
            copy(&mut tick.sliders, id, &el.slider);
            copy(&mut tick.dropdowns, id, &el.dropdown);
            copy(&mut tick.tabs, id, &el.tabs);
            copy(&mut tick.scrolls, id, &el.scroll);
        }
        tick
    }

    fn element(&self, id: EntityId) -> Option<&'s UiElement> {
        self.index.elements.get(&id).copied()
    }

    fn state(&mut self, id: EntityId) -> &mut UiState {
        self.states.entry(id).or_default()
    }

    fn sounds(&self, id: EntityId) -> Option<&'s UiSounds> {
        self.element(id)?.sounds.as_ref()
    }

    fn cue(&mut self, id: EntityId, pick: fn(&UiSounds) -> &Option<AssetRef>) {
        if let Some(cue) = self.sounds(id).and_then(|s| pick(s).clone()) {
            self.signals.push((id, Signal::Cue(cue)));
        }
    }

    /// The pointer in `id`'s root's UI space.
    fn point_in(&self, id: EntityId) -> Option<[f32; 2]> {
        let root = self.layout.nodes.get(&id)?.root;
        let map = self.layout.roots.iter().find(|r| r.entity == root)?.map;
        Some(map.unapply(self.viewport_point?))
    }

    /// Hover flags, hover ticks and tooltip anchors.
    fn hover(&mut self, hit: Option<&Hit>) {
        let widget = hit.and_then(|h| h.widget);
        let tip_host = hit.and_then(|h| {
            self.index
                .nearest(h.node, |e| e.tooltip.is_some() && !e.disabled())
        });
        if let (Some(w), Some(HitPart::Row(i))) = (widget, hit.map(|h| h.part)) {
            self.state(w).item = Some(i);
        }
        let ids: Vec<EntityId> = self.states.keys().copied().collect();
        for id in ids {
            let disabled = self.states[&id].disabled;
            let now = !disabled && (Some(id) == widget || Some(id) == tip_host);
            let was = self.states[&id].hovered;
            let delay = self
                .element(id)
                .and_then(|e| e.tooltip.as_ref())
                .map(|t| t.delay_ticks);
            let point = hit.map(|h| h.point);
            let s = self.state(id);
            s.hovered = now;
            s.hover_ticks = if now {
                s.hover_ticks.saturating_add(1)
            } else {
                0
            };
            if !now {
                s.tip = None;
            } else if let Some(delay) = delay
                && s.hover_ticks >= delay
                && s.tip.is_none()
            {
                s.tip = point;
            }
            if now && !was {
                self.cue(id, |s| &s.hover);
            }
        }
    }

    /// A press: closes lists it misses, presses and focuses the widget, and acts on press for
    /// dropdowns, sliders, tabs, scroll thumbs and `Press` buttons. Returns whether a list was
    /// closed (the click is used up).
    fn press(&mut self, hit: Option<&Hit>) -> bool {
        let widget = hit.and_then(|h| h.widget);
        let mut closed = false;
        for (id, dd) in self.dropdowns.iter_mut() {
            if dd.open && Some(*id) != widget {
                dd.open = false;
                closed = true;
            }
        }
        let (Some(hit), Some(w)) = (hit, widget) else {
            return closed;
        };
        let state = self.state(w);
        state.pressed = true;
        self.cue(w, |s| &s.press);
        if self.element(w).is_some_and(focusable) {
            self.focus(Some(w));
        }
        match hit.part {
            HitPart::Row(i) => self.pick(w, i),
            HitPart::Tab(i) => self.select_tab(w, i),
            HitPart::Thumb(bar) => self.grab(w, bar, hit.point),
            HitPart::Track(_) => {}
            HitPart::Body => {
                if let Some(dd) = self.dropdowns.get_mut(&w) {
                    dd.open = !dd.open;
                    let selected = dd.selected;
                    self.state(w).item = selected;
                }
                let press_button = self
                    .element(w)
                    .and_then(|e| e.button)
                    .is_some_and(|b| b.action_on == ActionOn::Press);
                if press_button {
                    self.signals.push((w, Signal::Pressed));
                }
            }
        }
        closed
    }

    /// Sliders follow the pointer; grabbed scroll thumbs move.
    fn drag(&mut self) {
        let pressed: Vec<EntityId> = self
            .states
            .iter()
            .filter(|(_, s)| s.pressed)
            .map(|(id, _)| *id)
            .collect();
        for id in pressed {
            let Some(q) = self.point_in(id) else { continue };
            if let (Some(slider), Some(Parts::Slider { track, .. })) =
                (self.sliders.get(&id).copied(), self.layout.parts.get(&id))
            {
                let v = slider_value_at(&slider, *track, q);
                self.set_value(id, v);
            }
            if let Some(grab) = self.states[&id].grab {
                self.drag_thumb(id, grab, q);
            }
        }
    }

    /// A release: fires the widget pressed under the pointer, and clears every press.
    fn release(&mut self, hit: Option<&Hit>) {
        let over = hit.and_then(|h| h.widget);
        let pressed: Vec<EntityId> = self
            .states
            .iter()
            .filter(|(_, s)| s.pressed)
            .map(|(id, _)| *id)
            .collect();
        for id in pressed {
            let s = self.state(id);
            s.pressed = false;
            s.grab = None;
            if Some(id) != over {
                continue;
            }
            self.cue(id, |s| &s.release);
            self.activate(id, true);
        }
    }

    /// What a release over the widget (or Accept) does to a button or toggle.
    fn activate(&mut self, id: EntityId, release: bool) {
        let el = self.element(id);
        if let Some(b) = el.and_then(|e| e.button)
            && (!release || b.action_on == ActionOn::Release)
        {
            self.signals.push((id, Signal::Pressed));
        }
        if self.toggles.contains_key(&id) {
            self.flip(id);
        }
    }

    /// Flips a toggle; in a group, turning one on turns the rest off, and the one that's on stays
    /// on unless it allows none.
    fn flip(&mut self, id: EntityId) {
        let Some(t) = self.toggles.get(&id).cloned() else {
            return;
        };
        if t.on && t.group.is_some() && !t.allow_none {
            return;
        }
        self.set_on(id, !t.on);
        if !t.on
            && let Some(group) = &t.group
        {
            let others: Vec<EntityId> = self
                .toggles
                .iter()
                .filter(|(o, x)| **o != id && x.on && x.group.as_ref() == Some(group))
                .map(|(o, _)| *o)
                .collect();
            for o in others {
                self.set_on(o, false);
            }
        }
        self.cue(id, |s| &s.toggle);
    }

    fn set_on(&mut self, id: EntityId, on: bool) {
        if let Some(t) = self.toggles.get_mut(&id) {
            t.on = on;
            self.signals.push((id, Signal::Toggled(on)));
        }
    }

    /// Sets a slider (snapped and clamped), signalling a change.
    fn set_value(&mut self, id: EntityId, v: f32) {
        let Some(slider) = self.sliders.get_mut(&id) else {
            return;
        };
        let v = slider.snap(v);
        if v != slider.value {
            slider.value = v;
            self.signals.push((id, Signal::Value(v)));
            self.cue(id, |s| &s.change);
        }
    }

    /// Picks dropdown row `i` and closes the list.
    fn pick(&mut self, id: EntityId, i: u32) {
        let Some(dd) = self.dropdowns.get_mut(&id) else {
            return;
        };
        dd.open = false;
        if dd.selected != Some(i) {
            dd.selected = Some(i);
            self.signals.push((id, Signal::Selected(i)));
            self.cue(id, |s| &s.change);
        }
        self.state(id).item = None;
    }

    /// Selects tab `i` (clamped to the pages).
    fn select_tab(&mut self, id: EntityId, i: u32) {
        let pages = self.element(id).map_or(0, |e| e.children.len() as u32);
        let Some(tabs) = self.tabs.get_mut(&id) else {
            return;
        };
        let i = i.min(pages.saturating_sub(1));
        if tabs.selected != i {
            tabs.selected = i;
            self.signals.push((id, Signal::Selected(i)));
            self.cue(id, |s| &s.change);
        }
    }

    /// Grabs a scroll thumb at `q`, remembering where on it.
    fn grab(&mut self, id: EntityId, which: Orientation, q: [f32; 2]) {
        let axis = axis(which);
        let Some(bar) = self.bar(id, axis) else {
            return;
        };
        let at = q[axis] - [bar.thumb.x, bar.thumb.y][axis];
        self.state(id).grab = Some(Grab { bar: which, at });
    }

    fn bar(&self, id: EntityId, axis: usize) -> Option<crate::parts::Bar> {
        match self.layout.parts.get(&id)? {
            Parts::Scroll {
                vertical,
                horizontal,
                ..
            } => {
                if axis == 1 {
                    *vertical
                } else {
                    *horizontal
                }
            }
            _ => None,
        }
    }

    /// Moves a grabbed thumb so the grab point follows `q`.
    fn drag_thumb(&mut self, id: EntityId, grab: Grab, q: [f32; 2]) {
        let axis = axis(grab.bar);
        let (Some(Parts::Scroll { content, .. }), Some(node), Some(bar)) = (
            self.layout.parts.get(&id),
            self.layout.nodes.get(&id),
            self.bar(id, axis),
        ) else {
            return;
        };
        let view = [node.rect.w, node.rect.h][axis];
        let offset = offset_for_thumb(&bar, axis, q[axis] - grab.at, view, content[axis]);
        if let Some(s) = self.scrolls.get_mut(&id) {
            s.offset[axis] = offset;
        }
    }

    /// The wheel scrolls the innermost scroll under the pointer (`step` pixels per line).
    fn wheel(&mut self, hit: &Hit, lines: [f32; 2]) {
        let Some(id) = self.index.nearest(hit.node, |e| e.scroll.is_some()) else {
            return;
        };
        let (
            Some(Parts::Scroll {
                content, offset, ..
            }),
            Some(node),
        ) = (self.layout.parts.get(&id), self.layout.nodes.get(&id))
        else {
            return;
        };
        let Some(scroll) = self.scrolls.get_mut(&id) else {
            return;
        };
        let moved = UiScroll {
            offset: [
                offset[0] - lines[0] * scroll.step,
                offset[1] - lines[1] * scroll.step,
            ],
            ..*scroll
        };
        scroll.offset = clamp_offset(&moved, [node.rect.w, node.rect.h], *content);
    }

    fn focused(&self) -> Option<EntityId> {
        self.states
            .iter()
            .find(|(_, s)| s.focused)
            .map(|(id, _)| *id)
    }

    /// Moves focus to `to`, signalling both ends.
    fn focus(&mut self, to: Option<EntityId>) {
        let from = self.focused();
        if from == to {
            return;
        }
        if let Some(f) = from {
            self.state(f).focused = false;
            self.signals.push((f, Signal::Focus(false)));
        }
        if let Some(t) = to {
            self.state(t).focused = true;
            self.signals.push((t, Signal::Focus(true)));
        }
    }

    /// Tab, arrows, Accept, Cancel and Page keys.
    fn keys(&mut self, keys: &Keys) {
        let from = self.focused();
        if keys.next || keys.prev {
            let to = step(&self.index, self.layout, from, keys.prev);
            self.focus(to);
            return;
        }
        let Some(f) = from else { return };
        if self.widget_keys(f, keys) {
            return;
        }
        let dir = [
            (keys.up, Dir::Up),
            (keys.down, Dir::Down),
            (keys.left, Dir::Left),
            (keys.right, Dir::Right),
        ]
        .into_iter()
        .find(|(down, _)| *down)
        .map(|(_, d)| d);
        if let Some(dir) = dir
            && let Some(to) = arrow(&self.index, self.layout, f, dir)
        {
            self.focus(Some(to));
        }
    }

    /// Keys the focused widget uses itself; true when it used them.
    fn widget_keys(&mut self, f: EntityId, keys: &Keys) -> bool {
        if let Some(dd) = self.dropdowns.get(&f).cloned() {
            return self.dropdown_keys(f, &dd, keys);
        }
        if let Some(slider) = self.sliders.get(&f).copied() {
            let (more, less) = match slider.orientation {
                Orientation::Horizontal => (keys.right, keys.left),
                Orientation::Vertical => (keys.up, keys.down),
            };
            let delta = if more {
                slider.key_step()
            } else if less {
                -slider.key_step()
            } else if keys.page_up {
                slider.page
            } else if keys.page_down {
                -slider.page
            } else {
                return false;
            };
            self.set_value(f, slider.value + delta);
            return true;
        }
        if let Some(tabs) = self.tabs.get(&f).cloned()
            && (keys.left || keys.right)
        {
            let i = if keys.right {
                tabs.selected + 1
            } else {
                tabs.selected.saturating_sub(1)
            };
            self.select_tab(f, i);
            return true;
        }
        if keys.accept {
            self.cue(f, |s| &s.press);
            self.activate(f, false);
            return true;
        }
        false
    }

    fn dropdown_keys(&mut self, f: EntityId, dd: &UiDropdown, keys: &Keys) -> bool {
        let last = (dd.options.len() as u32).saturating_sub(1);
        if !dd.open {
            if keys.accept {
                if let Some(d) = self.dropdowns.get_mut(&f) {
                    d.open = true;
                }
                self.state(f).item = dd.selected;
                return true;
            }
            return false;
        }
        let item = self
            .states
            .get(&f)
            .and_then(|s| s.item)
            .or(dd.selected)
            .unwrap_or(0);
        if keys.up || keys.down {
            let next = if keys.down {
                (item + 1).min(last)
            } else {
                item.saturating_sub(1)
            };
            self.state(f).item = Some(next);
        } else if keys.accept {
            self.pick(f, item.min(last));
        } else if keys.cancel {
            if let Some(d) = self.dropdowns.get_mut(&f) {
                d.open = false;
            }
            self.state(f).item = None;
        }
        true
    }

    /// Writes changed values and states back, then raises the signals in order.
    fn apply(self, world: &mut World) {
        for (id, el) in &self.index.elements {
            write(world, *id, &el.state, self.states.get(id));
            write(world, *id, &el.toggle, self.toggles.get(id));
            write(world, *id, &el.slider, self.sliders.get(id));
            write(world, *id, &el.dropdown, self.dropdowns.get(id));
            write(world, *id, &el.tabs, self.tabs.get(id));
            write(world, *id, &el.scroll, self.scrolls.get(id));
        }
        for (id, signal) in self.signals {
            match signal {
                Signal::Pressed => world.signal(id, UiPressed),
                Signal::Toggled(on) => world.signal(id, UiToggled { on }),
                Signal::Value(value) => world.signal(id, UiValueChanged { value }),
                Signal::Selected(index) => world.signal(id, UiSelected { index }),
                Signal::Focus(focused) => world.signal(id, UiFocusChanged { focused }),
                Signal::Cue(cue) => world.signal(
                    id,
                    PlayCue {
                        cue,
                        volume_db: 0.0,
                    },
                ),
            }
        }
    }
}

/// The axis index a bar scrolls (0: x, 1: y).
fn axis(bar: Orientation) -> usize {
    match bar {
        Orientation::Horizontal => 0,
        Orientation::Vertical => 1,
    }
}

fn copy<C: Clone>(map: &mut BTreeMap<EntityId, C>, id: EntityId, c: &Option<C>) {
    if let Some(c) = c {
        map.insert(id, c.clone());
    }
}

/// Inserts `now` when it differs from what the world had.
fn write<C: Component + Clone + PartialEq>(
    world: &mut World,
    id: EntityId,
    was: &Option<C>,
    now: Option<&C>,
) {
    if let Some(now) = now
        && was.as_ref() != Some(now)
    {
        let _ = world.insert(id, now.clone());
    }
}
