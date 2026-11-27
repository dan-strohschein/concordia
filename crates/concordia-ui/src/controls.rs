//! Interactive widgets (2.7b.2): buttons, toggles, sliders, dropdowns, tabs, tooltips, scrolling,
//! focus and sounds, plus `UiState` (what the pointer and keyboard are doing to a widget) and the
//! signals gameplay hears. Godot's `BaseButton`, `CheckBox`, `CheckButton`, `HSlider`,
//! `OptionButton`, `TabContainer`, `ScrollContainer` and WAI-ARIA's patterns, for semantics.

use concordia_assets::AssetRef;
use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::{UiError, finite, in_range};
use crate::widgets::Orientation;

/// Most dropdown options.
pub const MAX_OPTIONS: usize = 256;
/// Most tab titles.
pub const MAX_TITLES: usize = 64;
/// Longest option or title, in bytes.
pub const MAX_LABEL_BYTES: usize = 256;
/// Longest tooltip, in bytes.
pub const MAX_TOOLTIP_BYTES: usize = 4096;

/// What the pointer and keyboard are doing to a widget. The engine writes it, except
/// `disabled`; it is simulation state (hashed).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiState {
    /// The pointer is over it.
    pub hovered: bool,
    /// Pressed and not yet released.
    pub pressed: bool,
    /// It has keyboard focus.
    pub focused: bool,
    /// Ignores input and draws its theme's `disabled` style (set by you).
    pub disabled: bool,
    /// Ticks it has been hovered (tooltips).
    pub hover_ticks: u32,
    /// The highlighted item (open dropdown).
    pub item: Option<u32>,
    /// A grabbed scroll thumb.
    pub grab: Option<Grab>,
    /// Where its tooltip shows (UI space of its root).
    pub tip: Option<[f32; 2]>,
}

impl UiState {
    /// Checks the drag and tooltip points are finite.
    pub fn check(&self) -> Result<(), UiError> {
        if let Some(grab) = &self.grab {
            finite("UiState", "grab", &[grab.at])?;
        }
        if let Some(tip) = &self.tip {
            finite("UiState", "tip", tip)?;
        }
        Ok(())
    }
}

/// A grabbed scroll thumb: which bar, and how far along the thumb it was grabbed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Grab {
    /// The bar (`Horizontal` scrolls x).
    pub bar: Orientation,
    /// Pixels from the thumb's start.
    pub at: f32,
}

/// When a button acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum ActionOn {
    /// On release over the widget it was pressed on (WAI-ARIA).
    #[default]
    Release,
    /// On press.
    Press,
}

/// A clickable widget; raises `UiPressed`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiButton {
    /// When it fires.
    pub action_on: ActionOn,
}

impl UiButton {
    /// Always valid.
    pub fn check(&self) -> Result<(), UiError> {
        Ok(())
    }
}

/// How a toggle draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum ToggleLook {
    /// A box, filled when on.
    #[default]
    Checkbox,
    /// A track with a knob.
    Switch,
    /// The whole widget, in its theme's `pressed` style when on.
    Button,
}

/// A checkbox, switch or toggle button; in a `group`, a radio. Raises `UiToggled`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiToggle {
    /// On or off.
    pub on: bool,
    /// How it draws.
    pub look: ToggleLook,
    /// Toggles sharing a group keep at most one on.
    pub group: Option<String>,
    /// In a group, clicking the one that's on turns it off.
    pub allow_none: bool,
    /// Pixels between the box and the label.
    #[reflect(range(min = 0.0, max = 256.0))]
    pub label_gap: f32,
}

impl Default for UiToggle {
    fn default() -> Self {
        Self {
            on: false,
            look: ToggleLook::Checkbox,
            group: None,
            allow_none: false,
            label_gap: 6.0,
        }
    }
}

impl UiToggle {
    /// Checks the group name and the gap.
    pub fn check(&self) -> Result<(), UiError> {
        if let Some(group) = &self.group
            && !(1..=64).contains(&group.len())
        {
            return Err(UiError::field("UiToggle", "group", "1..=64 bytes"));
        }
        in_range("UiToggle", "label_gap", self.label_gap, 0.0, 256.0)
    }
}

/// A value picked along a track; raises `UiValueChanged`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiSlider {
    /// The value (clamped to min..max when drawn).
    pub value: f32,
    /// The low end.
    pub min: f32,
    /// The high end.
    pub max: f32,
    /// Snap step (0: continuous).
    pub step: f32,
    /// Its direction.
    pub orientation: Orientation,
    /// What Page Up / Page Down move.
    pub page: f32,
}

impl Default for UiSlider {
    fn default() -> Self {
        Self {
            value: 0.0,
            min: 0.0,
            max: 1.0,
            step: 0.0,
            orientation: Orientation::Horizontal,
            page: 0.1,
        }
    }
}

impl UiSlider {
    /// Checks finite values, min < max, step in 0..=(max − min) and a positive page.
    pub fn check(&self) -> Result<(), UiError> {
        const NAME: &str = "UiSlider";
        finite(NAME, "value", &[self.value])?;
        finite(NAME, "min", &[self.min])?;
        finite(NAME, "max", &[self.max])?;
        if self.min >= self.max {
            return Err(UiError::field(NAME, "max", "must be above min"));
        }
        in_range(NAME, "step", self.step, 0.0, self.max - self.min)?;
        in_range(NAME, "page", self.page, f32::MIN_POSITIVE, f32::MAX)
    }

    /// `v` snapped to the step from `min` (min + round((v − min) / step) · step), then clamped.
    pub fn snap(&self, v: f32) -> f32 {
        let snapped = match self.step > 0.0 {
            true => self.min + ((v - self.min) / self.step).round() * self.step,
            false => v,
        };
        snapped.clamp(self.min, self.max)
    }

    /// The value's position along the track, 0..1.
    pub fn fraction(&self) -> f32 {
        ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
    }

    /// One keyboard step: the step, or 1 % of the range when continuous.
    pub fn key_step(&self) -> f32 {
        match self.step > 0.0 {
            true => self.step,
            false => (self.max - self.min) / 100.0,
        }
    }
}

/// A button showing the selected option; opens a list. Raises `UiSelected`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiDropdown {
    /// The choices.
    pub options: Vec<String>,
    /// The chosen one.
    pub selected: Option<u32>,
    /// Whether the list is showing.
    pub open: bool,
    /// Most rows the list shows at once.
    #[reflect(range(min = 1.0, max = 64.0))]
    pub max_visible: u32,
}

impl Default for UiDropdown {
    fn default() -> Self {
        Self {
            options: Vec::new(),
            selected: None,
            open: false,
            max_visible: 8,
        }
    }
}

impl UiDropdown {
    /// Checks the options, the selection and the list's height.
    pub fn check(&self) -> Result<(), UiError> {
        const NAME: &str = "UiDropdown";
        labels(NAME, "options", &self.options, MAX_OPTIONS)?;
        if let Some(i) = self.selected
            && i as usize >= self.options.len()
        {
            return Err(UiError::field(
                NAME,
                "selected",
                format!("{i} is past the {} options", self.options.len()),
            ));
        }
        match (1..=64).contains(&self.max_visible) {
            true => Ok(()),
            false => Err(UiError::field(NAME, "max_visible", "must be in 1..=64")),
        }
    }
}

/// Pages with a row of titles; the children are the pages and only the selected one is laid out
/// and drawn. Raises `UiSelected`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiTabs {
    /// The shown page (clamped to the pages there are).
    pub selected: u32,
    /// One title per page.
    pub titles: Vec<String>,
}

impl UiTabs {
    /// Checks the titles.
    pub fn check(&self) -> Result<(), UiError> {
        labels("UiTabs", "titles", &self.titles, MAX_TITLES)
    }
}

/// Text shown near the pointer after the widget is hovered for a while.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiTooltip {
    /// What it says.
    pub text: String,
    /// Ticks of hovering before it shows.
    #[reflect(range(min = 0.0, max = 6000.0))]
    pub delay_ticks: u32,
}

impl Default for UiTooltip {
    fn default() -> Self {
        Self::new("")
    }
}

impl UiTooltip {
    /// A tooltip with the default delay (30 ticks).
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            delay_ticks: 30,
        }
    }

    /// Checks 1..=4096 bytes of text and a delay of at most 6000 ticks.
    pub fn check(&self) -> Result<(), UiError> {
        if !(1..=MAX_TOOLTIP_BYTES).contains(&self.text.len()) {
            return Err(UiError::field("UiTooltip", "text", "1..=4096 bytes"));
        }
        match self.delay_ticks <= 6000 {
            true => Ok(()),
            false => Err(UiError::field("UiTooltip", "delay_ticks", "at most 6000")),
        }
    }
}

/// When a scroll bar shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum ScrollBar {
    /// When the content overflows that axis.
    #[default]
    Auto,
    /// Always (the axis scrolls).
    Always,
    /// Never; the axis doesn't scroll and the content fits it.
    Never,
}

/// Clips its children and scrolls them by `offset` (wheel and bar drags).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiScroll {
    /// How far the content is scrolled, in pixels (clamped to the content).
    pub offset: [f32; 2],
    /// The horizontal bar.
    pub horizontal: ScrollBar,
    /// The vertical bar.
    pub vertical: ScrollBar,
    /// Pixels per wheel line.
    #[reflect(range(min = 0.001, max = 10000.0))]
    pub step: f32,
    /// Bar thickness in pixels.
    #[reflect(range(min = 1.0, max = 64.0))]
    pub bar_width: f32,
}

impl Default for UiScroll {
    fn default() -> Self {
        Self {
            offset: [0.0, 0.0],
            horizontal: ScrollBar::Never,
            vertical: ScrollBar::Auto,
            step: 40.0,
            bar_width: 8.0,
        }
    }
}

impl UiScroll {
    /// Checks a finite offset, a step in (0, 10000] and a bar width in 1..=64.
    pub fn check(&self) -> Result<(), UiError> {
        finite("UiScroll", "offset", &self.offset)?;
        if !(self.step > 0.0 && self.step <= 10000.0) {
            return Err(UiError::field("UiScroll", "step", "must be in (0, 10000]"));
        }
        in_range("UiScroll", "bar_width", self.bar_width, 1.0, 64.0)
    }

    /// Whether `axis` (0: x, 1: y) scrolls.
    pub fn scrolls(&self, axis: usize) -> bool {
        let bar = if axis == 0 {
            self.horizontal
        } else {
            self.vertical
        };
        bar != ScrollBar::Never
    }
}

/// Keyboard focus order and explicit neighbours.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiFocus {
    /// Tab order (widgets with one come first, ascending; then tree order).
    pub order: Option<i32>,
    /// Up arrow goes here.
    pub up: Option<EntityId>,
    /// Down arrow goes here.
    pub down: Option<EntityId>,
    /// Left arrow goes here.
    pub left: Option<EntityId>,
    /// Right arrow goes here.
    pub right: Option<EntityId>,
    /// Tab goes here.
    pub next: Option<EntityId>,
    /// Shift-Tab goes here.
    pub prev: Option<EntityId>,
}

impl UiFocus {
    /// Always valid (a missing neighbour falls back to the default order).
    pub fn check(&self) -> Result<(), UiError> {
        Ok(())
    }
}

/// Cues raised on the widget (`PlayCue`, unpositioned) as it is used.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiSounds {
    /// The pointer arrives.
    pub hover: Option<AssetRef>,
    /// Pressed.
    pub press: Option<AssetRef>,
    /// Released over it (a button fires).
    pub release: Option<AssetRef>,
    /// A toggle flips.
    pub toggle: Option<AssetRef>,
    /// A slider, dropdown or tab value changes.
    pub change: Option<AssetRef>,
}

impl UiSounds {
    /// Always valid (a missing cue is the audio module's warning).
    pub fn check(&self) -> Result<(), UiError> {
        Ok(())
    }
}

/// A button fired (signal, next tick).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "UI")]
pub struct UiPressed;

/// A toggle flipped (signal).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "UI")]
pub struct UiToggled {
    /// Its new state.
    pub on: bool,
}

/// A slider moved (signal).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "UI")]
pub struct UiValueChanged {
    /// The new value.
    pub value: f32,
}

/// A dropdown option or tab was chosen (signal).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "UI")]
pub struct UiSelected {
    /// Its index.
    pub index: u32,
}

/// Focus arrived or left (signal).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(signal, category = "UI")]
pub struct UiFocusChanged {
    /// Whether it now has focus.
    pub focused: bool,
}

/// Checks at most `most` labels of at most 256 bytes each.
fn labels(
    component: &'static str,
    field: &str,
    labels: &[String],
    most: usize,
) -> Result<(), UiError> {
    if labels.len() > most {
        return Err(UiError::field(
            component,
            field,
            format!("{} entries; at most {most}", labels.len()),
        ));
    }
    match labels.iter().position(|l| l.len() > MAX_LABEL_BYTES) {
        Some(i) => Err(UiError::field(
            component,
            format!("{field}/{i}"),
            "at most 256 bytes",
        )),
        None => Ok(()),
    }
}
