//! Drawing the interactive widgets (2.7b.2): each part becomes quads in the widget's text color
//! `c` (white by default), scroll bars go over their content, and open lists and tooltips go
//! above everything. Frame scale 1 and a 400 × 300 root, so pixels are UI units. Derived here:
//!
//! - **Checkbox** at (10, 10)–(210, 40), on: the box (10, 17, 16, 16) is an outline (border 1.5,
//!   radius 3), and the check is the box inset by 4, (14, 21, 8, 8), filled `c` (radius 2).
//! - **Switch**, on: the track (10, 17, 32, 16) filled `c` at 0.9 alpha (radius 8), the knob a
//!   12 px circle inset 2 from the right end: (10 + 32 − 2 − 12 = 28, 19, 12, 12) in
//!   `KNOB_DARK`. Off: the track at 0.3 alpha and the knob, in `c`, at x = 12.
//! - **Slider** (100, 100)–(300, 120) at 5 of 0..10: the track (108, 108, 184, 4) at 0.3 alpha,
//!   the filled part (108, 108, 92, 4) in `c`, the grabber (192, 102, 16, 16) a circle in `c`.
//! - **Dropdown** (100, 50)–(250, 80): the arrow box is 8 × 4 at (242, 63), drawn as a quad
//!   masked to a wedge about its bottom middle (246, 67) spanning 45° each side of up (atan(4/4)),
//!   which leaves a downward triangle.
//! - **Focus ring:** a focused widget gets a 2 px `FOCUS_COLOR` outline 2 px outside its box:
//!   (98, 98, 104, 44) for a button at (100, 100)–(200, 140).
//! - **Order:** a scroll's bars come after its children's quads; open lists and tooltips come
//!   after every node of every root.

mod common;

use common::Tree;
use concordia_api::{UiDrawList, UiFrame, UiMask, UiPaint, UiQuad};
use concordia_core::id::EntityId;
use concordia_ui::*;

const C: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

fn alpha(a: f32) -> [f32; 4] {
    [1.0, 1.0, 1.0, a]
}

fn screen(t: &mut Tree) -> EntityId {
    t.root(
        UiRoot {
            canvas: [400.0, 300.0],
            ..UiRoot::default()
        },
        UiNode::default(),
    )
}

fn pinned(l: f32, t: f32, r: f32, b: f32) -> UiNode {
    UiNode {
        position: Position::Anchored(Anchored {
            offsets: Edges::new(l, t, r, b),
            ..Anchored::default()
        }),
        ..UiNode::default()
    }
}

fn add<C2: concordia_terra::Component>(t: &mut Tree, id: EntityId, c: C2) {
    t.world.insert(id, c).unwrap();
}

fn draw(t: &Tree) -> UiDrawList {
    UiContext::default().draw(
        &t.world,
        &mut MemoryAssets::default(),
        &UiFrame::new(400, 300),
    )
}

#[track_caller]
fn find(list: &UiDrawList, rect: [f32; 4]) -> (usize, UiQuad) {
    let at = list
        .quads
        .iter()
        .position(|q| q.rect.iter().zip(rect).all(|(a, b)| (a - b).abs() < 1e-3))
        .unwrap_or_else(|| panic!("no quad at {rect:?}"));
    (at, list.quads[at].clone())
}

#[test]
fn checkbox_and_switch_draw_their_box_and_knob() {
    let mut t = Tree::new();
    let root = screen(&mut t);
    let id = t.child(root, pinned(10.0, 10.0, 210.0, 40.0));
    add(
        &mut t,
        id,
        UiToggle {
            on: true,
            ..UiToggle::default()
        },
    );
    let list = draw(&t);
    let (_, outline) = find(&list, [10.0, 17.0, 16.0, 16.0]);
    assert_eq!(outline.paint, UiPaint::None);
    assert_eq!((outline.border, outline.border_color), (1.5, C));
    assert_eq!(outline.radius, [3.0; 4]);
    let (_, check) = find(&list, [14.0, 21.0, 8.0, 8.0]);
    assert_eq!((check.paint, check.radius), (UiPaint::Solid(C), [2.0; 4]));

    for (on, track, knob_x, knob) in [
        (true, alpha(0.9), 28.0, KNOB_DARK),
        (false, alpha(0.3), 12.0, C),
    ] {
        let mut t = Tree::new();
        let root = screen(&mut t);
        let id = t.child(root, pinned(10.0, 10.0, 210.0, 40.0));
        add(
            &mut t,
            id,
            UiToggle {
                on,
                look: ToggleLook::Switch,
                ..UiToggle::default()
            },
        );
        let list = draw(&t);
        let (_, tr) = find(&list, [10.0, 17.0, 32.0, 16.0]);
        assert_eq!((tr.paint, tr.radius), (UiPaint::Solid(track), [8.0; 4]));
        let (_, k) = find(&list, [knob_x, 19.0, 12.0, 12.0]);
        assert_eq!((k.paint, k.radius), (UiPaint::Solid(knob), [6.0; 4]));
    }
}

#[test]
fn slider_draws_track_fill_and_grabber() {
    let mut t = Tree::new();
    let root = screen(&mut t);
    let id = t.child(root, pinned(100.0, 100.0, 300.0, 120.0));
    add(
        &mut t,
        id,
        UiSlider {
            value: 5.0,
            max: 10.0,
            ..UiSlider::default()
        },
    );
    let list = draw(&t);
    let (a, track) = find(&list, [108.0, 108.0, 184.0, 4.0]);
    let (b, fill) = find(&list, [108.0, 108.0, 92.0, 4.0]);
    let (c, grabber) = find(&list, [192.0, 102.0, 16.0, 16.0]);
    assert!(a < b && b < c, "track, fill, grabber");
    assert_eq!(track.paint, UiPaint::Solid(alpha(0.3)));
    assert_eq!(fill.paint, UiPaint::Solid(C));
    assert_eq!(
        (grabber.paint, grabber.radius),
        (UiPaint::Solid(C), [8.0; 4])
    );
}

#[test]
fn dropdown_arrow_is_a_masked_wedge() {
    let mut t = Tree::new();
    let root = screen(&mut t);
    let id = t.child(root, pinned(100.0, 50.0, 250.0, 80.0));
    add(
        &mut t,
        id,
        UiDropdown {
            options: vec!["Low".into()],
            selected: Some(0),
            ..UiDropdown::default()
        },
    );
    let list = draw(&t);
    let (_, arrow) = find(&list, [242.0, 63.0, 8.0, 4.0]);
    assert_eq!(arrow.paint, UiPaint::Solid(C));
    let quarter = std::f32::consts::FRAC_PI_4;
    match arrow.mask {
        UiMask::Angular {
            center,
            start,
            cw,
            ccw,
        } => {
            assert_eq!((center, start), ([246.0, 67.0], 0.0));
            assert!((cw - quarter).abs() < 1e-5 && (ccw - quarter).abs() < 1e-5);
        }
        UiMask::None => panic!("the arrow is masked"),
    }
}

#[test]
fn focused_widgets_draw_a_ring() {
    let mut t = Tree::new();
    let root = screen(&mut t);
    let id = t.child(root, pinned(100.0, 100.0, 200.0, 140.0));
    add(&mut t, id, UiButton::default());
    add(
        &mut t,
        id,
        UiState {
            focused: true,
            ..UiState::default()
        },
    );
    let list = draw(&t);
    let (_, ring) = find(&list, [98.0, 98.0, 104.0, 44.0]);
    assert_eq!(ring.paint, UiPaint::None);
    assert_eq!((ring.border, ring.border_color), (2.0, FOCUS_COLOR));
}

#[test]
fn bars_go_over_content_and_lists_and_tips_over_everything() {
    let mut t = Tree::new();
    let root = screen(&mut t);
    let scroll = t.child(
        root,
        UiNode {
            layout: Layout::Flex(Flex {
                direction: Direction::Column,
                ..Flex::default()
            }),
            ..pinned(0.0, 0.0, 200.0, 100.0)
        },
    );
    add(&mut t, scroll, UiScroll::default());
    for _ in 0..5 {
        let row = t.child(
            scroll,
            UiNode {
                height: Length::Px(50.0),
                ..UiNode::default()
            },
        );
        add(
            &mut t,
            row,
            UiStyle {
                background: Some(Background::Solid([0.2, 0.2, 0.2, 1.0])),
                ..UiStyle::default()
            },
        );
    }
    let dd = t.child(root, pinned(220.0, 10.0, 380.0, 40.0));
    add(
        &mut t,
        dd,
        UiDropdown {
            options: vec!["A".into(), "B".into()],
            selected: Some(0),
            open: true,
            ..UiDropdown::default()
        },
    );
    // A later sibling drawn after the dropdown's node, but under its list.
    let late = t.child(root, pinned(220.0, 200.0, 380.0, 290.0));
    add(
        &mut t,
        late,
        UiStyle {
            background: Some(Background::Solid([0.5, 0.0, 0.0, 1.0])),
            ..UiStyle::default()
        },
    );
    add(&mut t, late, UiTooltip::new("Hi"));
    add(
        &mut t,
        late,
        UiState {
            hovered: true,
            hover_ticks: 30,
            tip: Some([230.0, 210.0]),
            ..UiState::default()
        },
    );
    let list = draw(&t);
    let last_row = list
        .quads
        .iter()
        .rposition(|q| q.paint == UiPaint::Solid([0.2, 0.2, 0.2, 1.0]))
        .unwrap();
    // The thumb: 40 tall at the top of the track (192, 0, 8, 100).
    let (thumb, _) = find(&list, [192.0, 0.0, 8.0, 40.0]);
    assert!(thumb > last_row, "the bar is over the rows");
    let (late_bg, _) = find(&list, [220.0, 200.0, 160.0, 90.0]);
    let layout = {
        let scene = UiScene::gather(&t.world);
        compute_layout(
            &scene,
            &mut UiFonts::new(),
            &mut MemoryAssets::default(),
            [400.0, 300.0],
        )
    };
    let Some(Parts::Dropdown {
        list: Some(open), ..
    }) = layout.parts.get(&dd)
    else {
        panic!()
    };
    let r = open.rect;
    let (list_bg, _) = find(&list, [r.x, r.y, r.w, r.h]);
    assert!(list_bg > late_bg, "the list is over later siblings");
    let tip = &layout.tips[&late];
    let (tip_bg, _) = find(&list, [tip.rect.x, tip.rect.y, tip.rect.w, tip.rect.h]);
    assert!(tip_bg > list_bg, "tips are last");
}

/// Every interactive widget in one frame (the insta snapshot runs on Linux too).
#[test]
fn draw_list_snapshot_for_the_controls_fixture() {
    let (t, mut assets) = common::controls_fixture();
    let list = UiContext::default().draw(&t.world, &mut assets, &UiFrame::new(640, 360));
    insta::assert_snapshot!(common::summary(&list));
}
