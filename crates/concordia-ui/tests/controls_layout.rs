//! Layout of the interactive widgets (2.7b.2): styles that follow `UiState`, and each widget's
//! parts (`UiLayout::parts`, `UiLayout::tips`), which drawing and hit tests share. Every number
//! is derived by hand here.
//!
//! Fixture: a 400 × 300 root (stretch disabled) laid out for a 400 × 300 viewport, so UI space is
//! the viewport. The default text style is Noto Sans at 16 px; its line height is
//! (ascender − descender + lineGap) / unitsPerEm × 16, read from `hhea` by the test helper
//! (1069 + 293 + 0 = 1362 units: 21.792 px).
//!
//! - **State styles** resolve inline, then the state's style, then the base. The state is the
//!   first that holds of disabled, pressed, hovered, focused; a toggle drawn as a `Button` that
//!   is on counts as pressed.
//! - **Toggle:** the box side s is the font size (16) and sits at the content's left, centered
//!   vertically; a switch is 2s × s. The label starts s + gap (or 2s + gap) further right, as
//!   extra left padding. Anchored (10, 10)–(210, 40): box (10, 10 + (30 − 16)/2 = 17, 16, 16),
//!   content x = 10 + 16 + 6 = 32; a switch's box is 32 wide and its content x = 48.
//! - **Slider:** the grabber's side g = min(content height, 16); the track runs g/2 in from each
//!   end, 4 px thick, centered. (100, 100)–(300, 120), 5 of 0..10: g = 16, track (108, 108, 184,
//!   4), grabber x = 108 + 0.5 · 184 − 8 = 192: (192, 102, 16, 16). Vertical (100, 100)–(120,
//!   300), 2.5 of 0..10 (fraction 0.25, measured from the bottom): track (108, 108, 4, 184),
//!   grabber y = 292 − 0.25 · 184 − 8 = 238: (102, 238, 16, 16). In flow with `Auto` size, a
//!   horizontal slider is 160 × 20.
//! - **Dropdown:** rows are the line height + 8 (29.792); the list hangs under the face, as wide
//!   as it. (100, 50)–(250, 80) with 3 options: (100, 80, 150, 89.376), row i at y = 80 + i ·
//!   29.792. With `max_visible` 2 and row 2 highlighted, rows 1 and 2 show (first = 2 − 2 + 1).
//!   At (100, 260)–(250, 290) the list would end at 379.376 > 300, so it opens above: y = 260 −
//!   89.376 = 170.624.
//! - **Tabs:** the title row is the line height + 12 (33.792) tall; a title is its text width + 24
//!   wide, in order from the content's left. Only the selected page is laid out, below the row:
//!   (0, 33.792, 400, 266.208); a selection past the pages shows the last.
//! - **Scroll:** a 200 × 100 vertical scroll over a column of five 50 px rows: the content is 250
//!   tall, so the offset clamps to 0..150. At 400 it is 150: row 0 at y = −150, row 4 at y = 50.
//!   The bar overlays the right edge: track (192, 0, 8, 100), thumb 100 · 100/250 = 40 tall at
//!   y = 150/150 · (100 − 40) = 60. At 30, row 0 is at −30 and the thumb at 30/150 · 60 = 12.
//!   Children are clipped to the scroll's box.
//! - **Tooltip:** once hovered for `delay_ticks`, the tip shows at the `UiState.tip` point + (12,
//!   18), its text padded by 6, kept inside the root: at (50, 60) its box is at (62, 78); at
//!   (390, 290) it is pushed to (400 − w, 300 − h).

mod common;

use common::{Tree, assert_close, font_bytes, metrics};
use concordia_assets::AssetRef;
use concordia_core::id::EntityId;
use concordia_ui::*;

fn line_height() -> f32 {
    metrics(&font_bytes("NotoSans-Regular.ttf")).line_height(16.0)
}

fn screen() -> UiRoot {
    UiRoot {
        canvas: [400.0, 300.0],
        ..UiRoot::default()
    }
}

fn pinned(l: f32, t: f32, r: f32, b: f32) -> UiNode {
    UiNode {
        position: Position::Anchored(Anchored {
            offsets: Edges {
                left: l,
                top: t,
                right: r,
                bottom: b,
            },
            ..Anchored::default()
        }),
        ..UiNode::default()
    }
}

#[track_caller]
fn rect_is(got: Rect, want: [f32; 4]) {
    for (a, b) in [got.x, got.y, got.w, got.h].into_iter().zip(want) {
        assert!((a - b).abs() < 1e-3, "{got:?} != {want:?}");
    }
}

fn with<C: concordia_terra::Component>(t: &mut Tree, id: EntityId, c: C) -> EntityId {
    t.world.insert(id, c).unwrap();
    id
}

fn solid(r: f32) -> UiStyle {
    UiStyle {
        background: Some(Background::Solid([r, 0.0, 0.0, 1.0])),
        ..UiStyle::default()
    }
}

#[test]
fn theme_state_style_follows_ui_state() {
    let theme_ref: AssetRef = "builtin:theme/test".parse().unwrap();
    let mut assets = MemoryAssets::default();
    assets.insert_theme(
        &theme_ref,
        Theme {
            classes: [(
                "b".to_owned(),
                ThemeClass {
                    base: solid(0.1),
                    hover: solid(0.2),
                    pressed: solid(0.3),
                    disabled: solid(0.4),
                    focused: solid(0.5),
                },
            )]
            .into(),
        },
    );
    let state = |hovered, pressed, focused, disabled| UiState {
        hovered,
        pressed,
        focused,
        disabled,
        ..UiState::default()
    };
    let rows = [
        (UiState::default(), 0.1),
        (state(true, false, false, false), 0.2),
        (state(true, true, false, false), 0.3),
        (state(false, false, true, false), 0.5),
        (state(true, false, true, false), 0.2),
        (state(true, true, true, true), 0.4),
    ];
    for (ui_state, want) in rows {
        let mut t = Tree::new();
        let root = t.root(screen(), UiNode::default());
        let b = t.child(root, pinned(0.0, 0.0, 50.0, 20.0));
        with(
            &mut t,
            b,
            UiThemed {
                theme: theme_ref.clone(),
                class: "b".into(),
            },
        );
        with(&mut t, b, UiButton::default());
        with(&mut t, b, ui_state.clone());
        let l = t.layout_with([400.0, 300.0], &mut assets);
        assert_eq!(
            l.nodes[&b].style.background,
            Some(Background::Solid([want, 0.0, 0.0, 1.0])),
            "{ui_state:?}"
        );
    }
    // A Button-look toggle that is on draws pressed.
    let mut t = Tree::new();
    let root = t.root(screen(), UiNode::default());
    let b = t.child(root, pinned(0.0, 0.0, 50.0, 20.0));
    with(
        &mut t,
        b,
        UiThemed {
            theme: theme_ref.clone(),
            class: "b".into(),
        },
    );
    with(
        &mut t,
        b,
        UiToggle {
            on: true,
            look: ToggleLook::Button,
            ..UiToggle::default()
        },
    );
    let l = t.layout_with([400.0, 300.0], &mut assets);
    assert_eq!(
        l.nodes[&b].style.background,
        Some(Background::Solid([0.3, 0.0, 0.0, 1.0]))
    );
}

#[test]
fn toggle_box_sits_left_of_its_label() {
    for (look, width, content_x) in [
        (ToggleLook::Checkbox, 16.0, 32.0),
        (ToggleLook::Switch, 32.0, 48.0),
    ] {
        let mut t = Tree::new();
        let root = t.root(screen(), UiNode::default());
        let id = t.child(root, pinned(10.0, 10.0, 210.0, 40.0));
        with(
            &mut t,
            id,
            UiToggle {
                look,
                ..UiToggle::default()
            },
        );
        with(&mut t, id, UiText::plain("Sound"));
        let l = t.layout([400.0, 300.0]);
        let Some(Parts::Toggle { knob }) = l.parts.get(&id) else {
            panic!("{:?}", l.parts.get(&id));
        };
        rect_is(*knob, [10.0, 17.0, width, 16.0]);
        assert_close(l.nodes[&id].content.x, content_x);
    }
}

#[test]
fn slider_track_and_grabber_follow_the_value() {
    let cases = [
        (
            pinned(100.0, 100.0, 300.0, 120.0),
            Orientation::Horizontal,
            5.0,
            [108.0, 108.0, 184.0, 4.0],
            [192.0, 102.0, 16.0, 16.0],
        ),
        (
            pinned(100.0, 100.0, 120.0, 300.0),
            Orientation::Vertical,
            2.5,
            [108.0, 108.0, 4.0, 184.0],
            [102.0, 238.0, 16.0, 16.0],
        ),
    ];
    for (node, orientation, value, track_want, grabber_want) in cases {
        let mut t = Tree::new();
        let root = t.root(screen(), UiNode::default());
        let id = t.child(root, node);
        with(
            &mut t,
            id,
            UiSlider {
                value,
                max: 10.0,
                orientation,
                ..UiSlider::default()
            },
        );
        let l = t.layout([400.0, 300.0]);
        let Some(Parts::Slider { track, grabber }) = l.parts.get(&id) else {
            panic!("{:?}", l.parts.get(&id));
        };
        rect_is(*track, track_want);
        rect_is(*grabber, grabber_want);
    }
    // In flow and Auto-sized: 160 × 20.
    let mut t = Tree::new();
    let root = t.root(
        screen(),
        UiNode {
            layout: Layout::Flex(Flex {
                align_items: Align::Start,
                ..Flex::default()
            }),
            ..UiNode::default()
        },
    );
    let id = t.child(root, UiNode::default());
    with(&mut t, id, UiSlider::default());
    rect_is(
        t.layout([400.0, 300.0]).nodes[&id].rect,
        [0.0, 0.0, 160.0, 20.0],
    );
}

fn dropdown(t: &mut Tree, node: UiNode, max_visible: u32, item: Option<u32>) -> EntityId {
    let root = t.root(screen(), UiNode::default());
    let id = t.child(root, node);
    with(
        t,
        id,
        UiDropdown {
            options: vec!["Low".into(), "Medium".into(), "High".into()],
            selected: Some(0),
            open: true,
            max_visible,
        },
    );
    with(
        t,
        id,
        UiState {
            item,
            ..UiState::default()
        },
    )
}

#[test]
fn dropdown_list_hangs_under_the_face_and_flips_at_the_edge() {
    let row = line_height() + 8.0;
    let mut t = Tree::new();
    let id = dropdown(&mut t, pinned(100.0, 50.0, 250.0, 80.0), 8, None);
    let l = t.layout([400.0, 300.0]);
    let Some(Parts::Dropdown {
        list: Some(list), ..
    }) = l.parts.get(&id)
    else {
        panic!("{:?}", l.parts.get(&id));
    };
    rect_is(list.rect, [100.0, 80.0, 150.0, 3.0 * row]);
    let indices: Vec<u32> = list.rows.iter().map(|r| r.index).collect();
    assert_eq!(indices, [0, 1, 2]);
    for (i, r) in list.rows.iter().enumerate() {
        rect_is(r.rect, [100.0, 80.0 + i as f32 * row, 150.0, row]);
    }

    let mut t = Tree::new();
    let id = dropdown(&mut t, pinned(100.0, 50.0, 250.0, 80.0), 2, Some(2));
    let l = t.layout([400.0, 300.0]);
    let Some(Parts::Dropdown {
        list: Some(list), ..
    }) = l.parts.get(&id)
    else {
        panic!();
    };
    let indices: Vec<u32> = list.rows.iter().map(|r| r.index).collect();
    assert_eq!(indices, [1, 2]);
    rect_is(list.rect, [100.0, 80.0, 150.0, 2.0 * row]);

    let mut t = Tree::new();
    let id = dropdown(&mut t, pinned(100.0, 260.0, 250.0, 290.0), 8, None);
    let l = t.layout([400.0, 300.0]);
    let Some(Parts::Dropdown {
        list: Some(list), ..
    }) = l.parts.get(&id)
    else {
        panic!();
    };
    rect_is(list.rect, [100.0, 260.0 - 3.0 * row, 150.0, 3.0 * row]);
}

#[test]
fn tabs_lay_out_only_the_selected_page() {
    let header = line_height() + 12.0;
    for (selected, shown) in [(1, 1), (0, 0), (5, 1)] {
        let mut t = Tree::new();
        let root = t.root(screen(), UiNode::default());
        let tabs = t.child(
            root,
            UiNode {
                layout: Layout::Stack,
                ..pinned(0.0, 0.0, 400.0, 300.0)
            },
        );
        with(
            &mut t,
            tabs,
            UiTabs {
                selected,
                titles: vec!["One".into(), "Two".into()],
            },
        );
        let pages = [
            t.child(tabs, UiNode::default()),
            t.child(tabs, UiNode::default()),
        ];
        let l = t.layout([400.0, 300.0]);
        for (i, page) in pages.iter().enumerate() {
            assert_eq!(l.nodes.contains_key(page), i == shown, "{selected} {i}");
        }
        rect_is(
            l.nodes[&pages[shown]].rect,
            [0.0, header, 400.0, 300.0 - header],
        );
        let Some(Parts::Tabs { tabs: titles }) = l.parts.get(&tabs) else {
            panic!("{:?}", l.parts.get(&tabs));
        };
        assert_eq!(titles.len(), 2);
        let mut x = 0.0;
        for title in titles {
            rect_is(title.rect, [x, 0.0, title.text.width + 24.0, header]);
            x += title.rect.w;
        }
    }
}

/// A 200 × 100 vertical scroll with five 50 px rows; returns it, the rows and the layout.
fn scroll(offset: f32) -> (EntityId, Vec<EntityId>, UiLayout) {
    let mut t = Tree::new();
    let root = t.root(screen(), UiNode::default());
    let id = t.child(
        root,
        UiNode {
            layout: Layout::Flex(Flex {
                direction: Direction::Column,
                ..Flex::default()
            }),
            ..pinned(0.0, 0.0, 200.0, 100.0)
        },
    );
    with(
        &mut t,
        id,
        UiScroll {
            offset: [0.0, offset],
            ..UiScroll::default()
        },
    );
    let rows: Vec<EntityId> = (0..5)
        .map(|_| {
            t.child(
                id,
                UiNode {
                    height: Length::Px(50.0),
                    ..UiNode::default()
                },
            )
        })
        .collect();
    let l = t.layout([400.0, 300.0]);
    (id, rows, l)
}

#[test]
fn scroll_offset_clamps_to_content() {
    for (offset, clamped, row0, thumb_y) in [
        (400.0, 150.0, -150.0, 60.0),
        (30.0, 30.0, -30.0, 12.0),
        (-20.0, 0.0, 0.0, 0.0),
    ] {
        let (id, rows, l) = scroll(offset);
        rect_is(l.nodes[&rows[0]].rect, [0.0, row0, 200.0, 50.0]);
        rect_is(l.nodes[&rows[4]].rect, [0.0, row0 + 200.0, 200.0, 50.0]);
        let Some(Parts::Scroll {
            offset,
            content,
            vertical: Some(bar),
            horizontal: None,
        }) = l.parts.get(&id)
        else {
            panic!("{:?}", l.parts.get(&id));
        };
        assert_eq!(*offset, [0.0, clamped]);
        assert_eq!(*content, [200.0, 250.0]);
        rect_is(bar.track, [192.0, 0.0, 8.0, 100.0]);
        rect_is(bar.thumb, [192.0, thumb_y, 8.0, 40.0]);
    }
}

#[test]
fn scroll_clips_children() {
    let (_, rows, l) = scroll(30.0);
    for row in rows {
        rect_is(l.nodes[&row].clip, [0.0, 0.0, 200.0, 100.0]);
    }
}

#[test]
fn tooltip_shows_after_its_delay_inside_the_root() {
    let place = |tip: [f32; 2], hover_ticks: u32| {
        let mut t = Tree::new();
        let root = t.root(screen(), UiNode::default());
        let id = t.child(root, pinned(0.0, 0.0, 400.0, 300.0));
        with(
            &mut t,
            id,
            UiTooltip {
                text: "Saves".into(),
                delay_ticks: 3,
            },
        );
        with(
            &mut t,
            id,
            UiState {
                hovered: true,
                hover_ticks,
                tip: Some(tip),
                ..UiState::default()
            },
        );
        t.layout([400.0, 300.0]).tips.get(&id).cloned()
    };
    assert!(place([50.0, 60.0], 2).is_none(), "not before the delay");
    let tip = place([50.0, 60.0], 3).unwrap();
    rect_is(
        tip.rect,
        [62.0, 78.0, tip.text.width + 12.0, tip.text.height + 12.0],
    );
    let edge = place([390.0, 290.0], 3).unwrap();
    assert_close(edge.rect.x, 400.0 - edge.rect.w);
    assert_close(edge.rect.y, 300.0 - edge.rect.h);
}
