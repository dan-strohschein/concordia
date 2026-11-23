//! Fixtures of the interactive widgets (2.7b.2) for draw snapshots and goldens: each adds its
//! widgets to a root inside `area` (x, y), using one shared theme.
#![allow(dead_code)]

use concordia_assets::AssetRef;
use concordia_core::id::EntityId;
use concordia_ui::*;

use super::Tree;

const SLATE: [f32; 4] = [0.08, 0.09, 0.12, 1.0];
const PANEL: [f32; 4] = [0.05, 0.06, 0.08, 1.0];
const HOVER: [f32; 4] = [0.14, 0.17, 0.24, 1.0];
const ACCENT: [f32; 4] = [0.1, 0.45, 0.85, 1.0];
const MUTED: [f32; 4] = [0.5, 0.5, 0.55, 1.0];

pub fn theme_ref() -> AssetRef {
    "builtin:theme/controls".parse().unwrap()
}

fn style(bg: [f32; 4]) -> UiStyle {
    UiStyle {
        background: Some(Background::Solid(bg)),
        radius: Some([6.0; 4]),
        border: Some(Border {
            width: 1.0,
            color: [0.25, 0.28, 0.36, 1.0],
        }),
        ..UiStyle::default()
    }
}

/// `button`: slate, lighter when hovered, accent when pressed, muted text when disabled.
pub fn theme() -> Theme {
    Theme {
        classes: [(
            "button".to_owned(),
            ThemeClass {
                base: style(SLATE),
                hover: style(HOVER),
                pressed: style(ACCENT),
                disabled: UiStyle {
                    color: Some(MUTED),
                    ..style(PANEL)
                },
                focused: UiStyle::default(),
            },
        )]
        .into(),
    }
}

pub fn assets() -> MemoryAssets {
    let mut assets = MemoryAssets::default();
    assets.insert_theme(&theme_ref(), theme());
    assets
}

pub fn at(x: f32, y: f32, w: f32, h: f32) -> UiNode {
    UiNode {
        position: Position::Anchored(Anchored {
            offsets: Edges::new(x, y, x + w, y + h),
            ..Anchored::default()
        }),
        ..UiNode::default()
    }
}

fn put<C: concordia_terra::Component>(t: &mut Tree, id: EntityId, c: C) {
    t.world.insert(id, c).unwrap();
}

fn label(text: &str) -> UiText {
    UiText {
        align: TextAlign::Center,
        valign: VAlign::Center,
        ..UiText::plain(text)
    }
}

fn themed(t: &mut Tree, id: EntityId) {
    put(
        t,
        id,
        UiThemed {
            theme: theme_ref(),
            class: "button".into(),
        },
    );
}

/// Normal, hovered, pressed, disabled and focused buttons in a row.
pub fn button_states(t: &mut Tree, root: EntityId, [x, y]: [f32; 2]) {
    let states = [
        ("Play", UiState::default()),
        (
            "Hover",
            UiState {
                hovered: true,
                ..UiState::default()
            },
        ),
        (
            "Press",
            UiState {
                hovered: true,
                pressed: true,
                ..UiState::default()
            },
        ),
        (
            "Off",
            UiState {
                disabled: true,
                ..UiState::default()
            },
        ),
        (
            "Focus",
            UiState {
                focused: true,
                ..UiState::default()
            },
        ),
    ];
    for (i, (text, state)) in states.into_iter().enumerate() {
        let id = t.child(root, at(x + i as f32 * 76.0, y, 68.0, 32.0));
        put(t, id, UiButton::default());
        themed(t, id);
        put(t, id, label(text));
        put(t, id, state);
    }
}

/// Checkbox, switch and button looks, off then on.
pub fn toggle_looks(t: &mut Tree, root: EntityId, [x, y]: [f32; 2]) {
    let looks = [
        (ToggleLook::Checkbox, "Sound"),
        (ToggleLook::Switch, "Music"),
        (ToggleLook::Button, "Mute"),
    ];
    for (row, (look, text)) in looks.into_iter().enumerate() {
        for (col, on) in [false, true].into_iter().enumerate() {
            let id = t.child(
                root,
                at(x + col as f32 * 150.0, y + row as f32 * 36.0, 140.0, 28.0),
            );
            put(
                t,
                id,
                UiToggle {
                    on,
                    look,
                    ..UiToggle::default()
                },
            );
            put(t, id, UiText::plain(text));
            if look == ToggleLook::Button {
                themed(t, id);
                put(t, id, label(text));
            }
        }
    }
}

/// A continuous slider at 0.25, a stepped one at 7 of 0..10, and a vertical one at 0.6.
pub fn sliders(t: &mut Tree, root: EntityId, [x, y]: [f32; 2]) {
    let rows = [
        (
            at(x, y, 200.0, 20.0),
            0.25,
            1.0,
            0.0,
            Orientation::Horizontal,
        ),
        (
            at(x, y + 32.0, 200.0, 20.0),
            7.0,
            10.0,
            1.0,
            Orientation::Horizontal,
        ),
        (
            at(x + 220.0, y, 20.0, 120.0),
            0.6,
            1.0,
            0.0,
            Orientation::Vertical,
        ),
    ];
    for (node, value, max, step, orientation) in rows {
        let id = t.child(root, node);
        put(
            t,
            id,
            UiSlider {
                value,
                max,
                step,
                orientation,
                ..UiSlider::default()
            },
        );
    }
}

/// An open dropdown, its third option highlighted.
pub fn dropdown_open(t: &mut Tree, root: EntityId, [x, y]: [f32; 2]) {
    let id = t.child(
        root,
        UiNode {
            padding: Edges::new(10.0, 0.0, 10.0, 0.0),
            ..at(x, y, 180.0, 32.0)
        },
    );
    put(
        t,
        id,
        UiDropdown {
            options: vec![
                "Easy".into(),
                "Normal".into(),
                "Hard".into(),
                "Nightmare".into(),
            ],
            selected: Some(1),
            open: true,
            ..UiDropdown::default()
        },
    );
    themed(t, id);
    put(
        t,
        id,
        UiState {
            item: Some(2),
            ..UiState::default()
        },
    );
}

/// Tabs over three pages, the second selected.
pub fn tabs(t: &mut Tree, root: EntityId, [x, y]: [f32; 2]) {
    let id = t.child(
        root,
        UiNode {
            layout: Layout::Stack,
            ..at(x, y, 280.0, 140.0)
        },
    );
    put(
        t,
        id,
        UiTabs {
            selected: 1,
            titles: vec!["Video".into(), "Audio".into(), "Controls".into()],
        },
    );
    put(t, id, style(PANEL));
    for (i, color) in [
        [0.3, 0.1, 0.1, 1.0],
        [0.1, 0.25, 0.12, 1.0],
        [0.1, 0.1, 0.3, 1.0],
    ]
    .into_iter()
    .enumerate()
    {
        let page = t.child(
            id,
            UiNode {
                margin: Edges::all(8.0),
                ..UiNode::default()
            },
        );
        put(
            t,
            page,
            UiStyle {
                background: Some(Background::Solid(color)),
                radius: Some([4.0; 4]),
                ..UiStyle::default()
            },
        );
        put(t, page, label(&format!("Page {}", i + 1)));
    }
}

/// A scroll over eight labelled rows, scrolled by 60.
pub fn scroll(t: &mut Tree, root: EntityId, [x, y]: [f32; 2]) {
    let id = t.child(
        root,
        UiNode {
            layout: Layout::Flex(Flex {
                direction: Direction::Column,
                ..Flex::default()
            }),
            gap: [0.0, 4.0],
            padding: Edges::all(4.0),
            ..at(x, y, 200.0, 140.0)
        },
    );
    put(
        t,
        id,
        UiScroll {
            offset: [0.0, 60.0],
            ..UiScroll::default()
        },
    );
    put(t, id, style(PANEL));
    for i in 0..8 {
        let row = t.child(
            id,
            UiNode {
                height: Length::Px(28.0),
                padding: Edges::new(8.0, 0.0, 8.0, 0.0),
                ..UiNode::default()
            },
        );
        put(
            t,
            row,
            UiStyle {
                background: Some(Background::Solid(SLATE)),
                radius: Some([4.0; 4]),
                ..UiStyle::default()
            },
        );
        put(
            t,
            row,
            UiText {
                valign: VAlign::Center,
                ..UiText::plain(format!("Save slot {}", i + 1))
            },
        );
    }
}

/// A button with its tooltip showing.
pub fn tooltip(t: &mut Tree, root: EntityId, [x, y]: [f32; 2]) {
    let id = t.child(root, at(x, y, 90.0, 32.0));
    put(t, id, UiButton::default());
    themed(t, id);
    put(t, id, label("Save"));
    put(t, id, UiTooltip::new("Saves to the selected slot"));
    put(
        t,
        id,
        UiState {
            hovered: true,
            hover_ticks: 30,
            tip: Some([x + 40.0, y + 16.0]),
            ..UiState::default()
        },
    );
}

/// A 640 × 360 root of every interactive widget.
pub fn controls_fixture() -> (Tree, MemoryAssets) {
    let mut t = Tree::new();
    let root = t.root(
        UiRoot {
            canvas: [640.0, 360.0],
            ..UiRoot::default()
        },
        UiNode::default(),
    );
    button_states(&mut t, root, [10.0, 10.0]);
    toggle_looks(&mut t, root, [10.0, 56.0]);
    sliders(&mut t, root, [330.0, 56.0]);
    tabs(&mut t, root, [10.0, 170.0]);
    scroll(&mut t, root, [300.0, 200.0]);
    tooltip(&mut t, root, [540.0, 140.0]);
    dropdown_open(&mut t, root, [420.0, 10.0]);
    (t, assets())
}
