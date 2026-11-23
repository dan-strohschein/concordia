//! Drawing text boxes, spin boxes and lists (2.7b.3). Frame scale 1 on a 400 × 300 root, so
//! pixels are UI units; the widget's text color `c` is white by default.
//!
//! - **Box** (10, 10)–(210, 40), focused, "hello" with 1..5 selected: the selection (the
//!   layout's rect) is filled with `FOCUS_COLOR` at 0.35 alpha before the glyphs, the caret
//!   (the layout's rect) in `c` after them; every one is clipped to the content box,
//!   (10, 10)–(210, 40).
//! - **Placeholder:** its glyphs draw at half opacity.
//! - **Spin box** (10, 10)–(110, 40): each arrow half (20 × 15) holds an 8 × 4 triangle at its
//!   center: up (96, 15.5) masked about its top middle (100, 15.5) pointing down the wedge
//!   (start π), down (96, 30.5) masked about its bottom middle (100, 34.5) (start 0).
//! - **List:** each selected item gets a `FOCUS_COLOR` overlay at 0.3 alpha after the items.

mod common;

use common::Tree;
use concordia_api::{UiDrawList, UiFrame, UiMask, UiPaint};
use concordia_core::id::EntityId;
use concordia_ui::*;

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

fn draw(t: &Tree) -> (UiDrawList, UiLayout) {
    let list = UiContext::default().draw(
        &t.world,
        &mut MemoryAssets::default(),
        &UiFrame::new(400, 300),
    );
    let layout = t.layout([400.0, 300.0]);
    (list, layout)
}

fn alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], c[3] * a]
}

#[track_caller]
fn at(list: &UiDrawList, r: Rect) -> usize {
    list.quads
        .iter()
        .position(|q| {
            q.rect
                .iter()
                .zip([r.x, r.y, r.w, r.h])
                .all(|(a, b)| (a - b).abs() < 1e-3)
        })
        .unwrap_or_else(|| panic!("no quad at {r:?}"))
}

fn text_box(t: &mut Tree, b: UiTextBox, focused: bool) -> EntityId {
    let root = screen(t);
    let id = t.child(root, pinned(10.0, 10.0, 210.0, 40.0));
    t.world.insert(id, b).unwrap();
    t.world
        .insert(
            id,
            UiState {
                focused,
                ..UiState::default()
            },
        )
        .unwrap();
    id
}

#[test]
fn text_box_draws_selection_then_text_then_caret_clipped() {
    let mut t = Tree::new();
    let id = text_box(
        &mut t,
        UiTextBox {
            text: "hello".into(),
            caret: 5,
            anchor: 1,
            ..UiTextBox::default()
        },
        true,
    );
    let (list, layout) = draw(&t);
    let Some(Parts::TextBox(p)) = layout.parts.get(&id) else {
        panic!()
    };
    let sel = at(&list, p.selection[0]);
    let caret = at(&list, p.caret.unwrap());
    assert_eq!(
        list.quads[sel].paint,
        UiPaint::Solid(alpha(FOCUS_COLOR, 0.35))
    );
    assert_eq!(list.quads[caret].paint, UiPaint::Solid([1.0; 4]));
    let glyphs: Vec<usize> = (0..list.quads.len())
        .filter(|i| matches!(list.quads[*i].paint, UiPaint::Glyph { .. }))
        .collect();
    assert_eq!(glyphs.len(), 5);
    assert!(glyphs.iter().all(|g| sel < *g && *g < caret));
    for i in glyphs.iter().copied().chain([sel, caret]) {
        assert_eq!(list.quads[i].clip, [10.0, 10.0, 210.0, 40.0]);
    }
}

#[test]
fn placeholders_draw_at_half_opacity() {
    let mut t = Tree::new();
    text_box(
        &mut t,
        UiTextBox {
            placeholder: "Name".into(),
            ..UiTextBox::default()
        },
        false,
    );
    let (list, _) = draw(&t);
    let glyphs: Vec<f32> = list
        .quads
        .iter()
        .filter(|q| matches!(q.paint, UiPaint::Glyph { .. }))
        .map(|q| q.opacity)
        .collect();
    assert_eq!(glyphs, [0.5; 4]);
}

#[test]
fn spin_arrows_are_masked_triangles() {
    let mut t = Tree::new();
    let root = screen(&mut t);
    let id = t.child(root, pinned(10.0, 10.0, 110.0, 40.0));
    t.world.insert(id, UiTextBox::default()).unwrap();
    t.world.insert(id, UiSpinBox::default()).unwrap();
    let (list, _) = draw(&t);
    let half = 4.0f32.atan2(4.0);
    for (rect, center, start) in [
        (
            Rect::new(96.0, 15.5, 8.0, 4.0),
            [100.0, 15.5],
            std::f32::consts::PI,
        ),
        (Rect::new(96.0, 30.5, 8.0, 4.0), [100.0, 34.5], 0.0),
    ] {
        let q = &list.quads[at(&list, rect)];
        match q.mask {
            UiMask::Angular {
                center: c,
                start: s,
                cw,
                ccw,
            } => {
                assert_eq!((c, s), (center, start));
                assert!((cw - half).abs() < 1e-5 && (ccw - half).abs() < 1e-5);
            }
            UiMask::None => panic!("arrows are masked"),
        }
    }
}

#[test]
fn selected_list_items_are_highlighted_over_the_items() {
    let mut t = Tree::new();
    let root = screen(&mut t);
    let l = t.child(
        root,
        UiNode {
            layout: Layout::Flex(Flex {
                direction: Direction::Column,
                ..Flex::default()
            }),
            ..pinned(10.0, 10.0, 210.0, 210.0)
        },
    );
    t.world
        .insert(
            l,
            UiList {
                selected: vec![1, 3],
                multi: true,
            },
        )
        .unwrap();
    let items: Vec<EntityId> = (0..5)
        .map(|_| {
            let c = t.child(
                l,
                UiNode {
                    height: Length::Px(40.0),
                    ..UiNode::default()
                },
            );
            t.world
                .insert(
                    c,
                    UiStyle {
                        background: Some(Background::Solid([0.2, 0.2, 0.2, 1.0])),
                        ..UiStyle::default()
                    },
                )
                .unwrap();
            c
        })
        .collect();
    let (list, layout) = draw(&t);
    let last_item = at(&list, layout.nodes[&items[4]].rect);
    let highlights: Vec<usize> = list
        .quads
        .iter()
        .enumerate()
        .filter(|(_, q)| q.paint == UiPaint::Solid(alpha(FOCUS_COLOR, 0.3)))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(highlights.len(), 2);
    assert!(highlights.iter().all(|h| *h > last_item));
    for (h, item) in highlights.iter().zip([1, 3]) {
        let want = layout.nodes[&items[item]].rect;
        let r = list.quads[*h].rect;
        assert_eq!(r, [want.x, want.y, want.w, want.h]);
    }
}

/// Text boxes, a spin box and a list in one frame (the insta snapshot runs on Linux too).
#[test]
fn draw_list_snapshot_for_the_text_fixture() {
    let (t, mut assets) = common::controls::text_fixture();
    let list = UiContext::default().draw(&t.world, &mut assets, &UiFrame::new(640, 360));
    insta::assert_snapshot!(common::summary(&list));
}
