//! A snapshot of a world's UI trees: each `UiRoot` and its `Parent` descendants that carry UI
//! components, siblings in (`order`, id) order, with invalid components reported and dropped.

use std::collections::{BTreeMap, BTreeSet};

use concordia_core::id::EntityId;
use concordia_scene::builtin::Parent;
use concordia_terra::{Component, World};

use crate::controls::{
    UiButton, UiDropdown, UiFocus, UiScroll, UiSlider, UiSounds, UiState, UiTabs, UiToggle,
    UiTooltip,
};
use crate::error::UiError;
use crate::node::UiNode;
use crate::root::UiRoot;
use crate::style::{UiStyle, UiThemed};
use crate::widgets::{UiImage, UiNinePatch, UiProgress, UiSeparator, UiText};

/// One widget and its children.
#[derive(Debug, Clone, PartialEq)]
pub struct UiElement {
    /// Its entity.
    pub entity: EntityId,
    /// Its box (the default when absent or invalid).
    pub node: UiNode,
    /// Its inline style.
    pub style: Option<UiStyle>,
    /// Its theme class.
    pub themed: Option<UiThemed>,
    /// A label.
    pub text: Option<UiText>,
    /// A picture.
    pub image: Option<UiImage>,
    /// A nine-patch.
    pub nine: Option<UiNinePatch>,
    /// A progress bar.
    pub progress: Option<UiProgress>,
    /// A separator.
    pub separator: Option<UiSeparator>,
    /// What input is doing to it.
    pub state: Option<UiState>,
    /// A button.
    pub button: Option<UiButton>,
    /// A checkbox, switch or toggle button.
    pub toggle: Option<UiToggle>,
    /// A slider.
    pub slider: Option<UiSlider>,
    /// A dropdown.
    pub dropdown: Option<UiDropdown>,
    /// Tabs (its children are the pages).
    pub tabs: Option<UiTabs>,
    /// A tooltip.
    pub tooltip: Option<UiTooltip>,
    /// A scroll container.
    pub scroll: Option<UiScroll>,
    /// Focus order and neighbours.
    pub focus: Option<UiFocus>,
    /// Sounds.
    pub sounds: Option<UiSounds>,
    /// Children in layout order.
    pub children: Vec<UiElement>,
}

impl UiElement {
    /// Whether it carries any UI component besides `UiNode`.
    pub fn has_widget(&self) -> bool {
        self.style.is_some()
            || self.themed.is_some()
            || self.text.is_some()
            || self.image.is_some()
            || self.nine.is_some()
            || self.progress.is_some()
            || self.separator.is_some()
            || self.state.is_some()
            || self.is_interactive()
            || self.tooltip.is_some()
            || self.scroll.is_some()
            || self.focus.is_some()
            || self.sounds.is_some()
    }

    /// Whether it takes clicks or keys (button, toggle, slider, dropdown or tabs).
    pub fn is_interactive(&self) -> bool {
        self.button.is_some()
            || self.toggle.is_some()
            || self.slider.is_some()
            || self.dropdown.is_some()
            || self.tabs.is_some()
    }

    /// Whether it is disabled.
    pub fn disabled(&self) -> bool {
        self.state.as_ref().is_some_and(|s| s.disabled)
    }
}

/// A root and its tree.
#[derive(Debug, Clone, PartialEq)]
pub struct UiTree {
    /// The root's settings.
    pub root: UiRoot,
    /// The root's own element (it spans the UI space).
    pub element: UiElement,
}

/// Every UI tree in a world, roots by (`z`, id).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiScene {
    /// The trees.
    pub trees: Vec<UiTree>,
    /// Components that failed their checks (dropped), by entity.
    pub problems: Vec<(EntityId, UiError)>,
}

impl UiScene {
    /// Reads every UI tree in `world`.
    pub fn gather(world: &World) -> UiScene {
        let ids = world.entity_ids_sorted();
        let mut children: BTreeMap<EntityId, Vec<EntityId>> = BTreeMap::new();
        for &id in &ids {
            if let Some(Parent(parent)) = world.get::<Parent>(id) {
                children.entry(*parent).or_default().push(id);
            }
        }
        let mut gather = Gather {
            world,
            children,
            seen: BTreeSet::new(),
            problems: Vec::new(),
        };
        let mut trees = Vec::new();
        for &id in &ids {
            let Some(root) = world.get::<UiRoot>(id) else {
                continue;
            };
            if let Err(e) = root.check() {
                gather.problems.push((id, e));
                continue;
            }
            if let Some(element) = gather.element(id, true) {
                trees.push(UiTree {
                    root: *root,
                    element,
                });
            }
        }
        trees.sort_by_key(|t| (t.root.z, t.element.entity));
        UiScene {
            trees,
            problems: gather.problems,
        }
    }

    /// Whether there is no UI.
    pub fn is_empty(&self) -> bool {
        self.trees.is_empty()
    }
}

struct Gather<'w> {
    world: &'w World,
    children: BTreeMap<EntityId, Vec<EntityId>>,
    seen: BTreeSet<EntityId>,
    problems: Vec<(EntityId, UiError)>,
}

impl Gather<'_> {
    /// `id`'s component if it passes `check`; a failure is reported and the component dropped.
    fn checked<C: Component + Clone>(
        &mut self,
        id: EntityId,
        check: impl Fn(&C) -> Result<(), UiError>,
    ) -> Option<C> {
        let c = self.world.get::<C>(id)?;
        match check(c) {
            Ok(()) => Some(c.clone()),
            Err(e) => {
                self.problems.push((id, e));
                None
            }
        }
    }

    fn element(&mut self, id: EntityId, is_root: bool) -> Option<UiElement> {
        if !self.seen.insert(id) || (!is_root && self.world.get::<UiRoot>(id).is_some()) {
            return None;
        }
        let present = self.world.get::<UiNode>(id).is_some();
        let node = self.checked::<UiNode>(id, UiNode::check);
        let mut element = UiElement {
            entity: id,
            node: node.unwrap_or_default(),
            style: self.checked(id, UiStyle::check),
            themed: self.checked(id, UiThemed::check),
            text: self.checked(id, UiText::check),
            image: self.checked(id, UiImage::check),
            nine: self.checked(id, UiNinePatch::check),
            progress: self.checked(id, UiProgress::check),
            separator: self.checked(id, UiSeparator::check),
            state: self.checked(id, UiState::check),
            button: self.checked(id, UiButton::check),
            toggle: self.checked(id, UiToggle::check),
            slider: self.checked(id, UiSlider::check),
            dropdown: self.checked(id, UiDropdown::check),
            tabs: self.checked(id, UiTabs::check),
            tooltip: self.checked(id, UiTooltip::check),
            scroll: self.checked(id, UiScroll::check),
            focus: self.checked(id, UiFocus::check),
            sounds: self.checked(id, UiSounds::check),
            children: Vec::new(),
        };
        let any = present || element.has_widget();
        if (!is_root && !any) || !element.node.visible {
            return None;
        }
        let kids = self.children.get(&id).cloned().unwrap_or_default();
        element.children = kids
            .into_iter()
            .filter_map(|k| self.element(k, false))
            .collect();
        element.children.sort_by_key(|c| (c.node.order, c.entity));
        Some(element)
    }
}
