//! Layout of UI trees from a world: taffy flex, grid and stack (CSS Flexbox and Grid), anchored
//! islands (Godot anchors), canvas stretch, and intrinsic text and image sizes. Every rect is
//! derived by hand in its test's comment.

mod common;

use common::{Tree, assert_close, font_bytes, metrics};
use concordia_core::id::EntityId;
use concordia_ui::*;

fn screen(w: f32, h: f32) -> UiRoot {
    UiRoot {
        canvas: [w, h],
        ..UiRoot::default()
    }
}

fn sized(w: f32, h: f32) -> UiNode {
    UiNode {
        width: Length::Px(w),
        height: Length::Px(h),
        ..UiNode::default()
    }
}

fn flex(direction: Direction, justify: Justify, align_items: Align) -> Layout {
    Layout::Flex(Flex {
        direction,
        justify,
        align_items,
        ..Flex::default()
    })
}

#[track_caller]
fn rect_is(layout: &UiLayout, id: EntityId, want: Rect) {
    let got = layout.nodes[&id].rect;
    for (a, b) in [
        (got.x, want.x),
        (got.y, want.y),
        (got.w, want.w),
        (got.h, want.h),
    ] {
        assert!((a - b).abs() < 1e-3, "{got:?} != {want:?}");
    }
}

/// Root 400 × 300, padding 10, column gap 5, a row aligned to the start: children 50 × 20,
/// 60 × 20 and 70 × 30 at x = 10, 10 + 50 + 5 = 65, 65 + 60 + 5 = 130, all at y = 10. Centered,
/// the row is 50 + 60 + 70 + 2·5 = 190 wide in a 380 content box, so it starts at
/// 10 + (380 − 190)/2 = 105: x = 105, 160, 225.
#[test]
fn flex_row_places_children_with_gap_and_padding() {
    for (justify, xs) in [
        (Justify::Start, [10.0, 65.0, 130.0]),
        (Justify::Center, [105.0, 160.0, 225.0]),
    ] {
        let mut t = Tree::new();
        let root = t.root(
            screen(400.0, 300.0),
            UiNode {
                padding: Edges::all(10.0),
                gap: [5.0, 0.0],
                layout: flex(Direction::Row, justify, Align::Start),
                ..UiNode::default()
            },
        );
        let kids = [
            t.child(root, sized(50.0, 20.0)),
            t.child(root, sized(60.0, 20.0)),
            t.child(root, sized(70.0, 30.0)),
        ];
        let l = t.layout([400.0, 300.0]);
        rect_is(&l, root, Rect::new(0.0, 0.0, 400.0, 300.0));
        for ((id, x), (w, h)) in kids
            .iter()
            .zip(xs)
            .zip([(50.0, 20.0), (60.0, 20.0), (70.0, 30.0)])
        {
            rect_is(&l, *id, Rect::new(x, 10.0, w, h));
        }
        assert_eq!(l.nodes[&root].content, Rect::new(10.0, 10.0, 380.0, 280.0));
    }
}

/// A column justified to the end and centered: heights 20 + 20 + 30 plus two 5 gaps is 80, so
/// it starts at 10 + 280 − 80 = 210: y = 210, 235, 260; x = 10 + (380 − w)/2 = 175, 170, 165.
/// In a row, a child with grow 1 beside a fixed 100 takes 380 − 100 = 280.
#[test]
fn flex_column_justify_align_and_grow() {
    let mut t = Tree::new();
    let root = t.root(
        screen(400.0, 300.0),
        UiNode {
            padding: Edges::all(10.0),
            gap: [0.0, 5.0],
            layout: flex(Direction::Column, Justify::End, Align::Center),
            ..UiNode::default()
        },
    );
    let a = t.child(root, sized(50.0, 20.0));
    let b = t.child(root, sized(60.0, 20.0));
    let c = t.child(root, sized(70.0, 30.0));
    let l = t.layout([400.0, 300.0]);
    rect_is(&l, a, Rect::new(175.0, 210.0, 50.0, 20.0));
    rect_is(&l, b, Rect::new(170.0, 235.0, 60.0, 20.0));
    rect_is(&l, c, Rect::new(165.0, 260.0, 70.0, 30.0));

    let mut t = Tree::new();
    let root = t.root(
        screen(400.0, 300.0),
        UiNode {
            padding: Edges::all(10.0),
            layout: flex(Direction::Row, Justify::Start, Align::Stretch),
            ..UiNode::default()
        },
    );
    let fixed = t.child(
        root,
        UiNode {
            width: Length::Px(100.0),
            ..UiNode::default()
        },
    );
    let grows = t.child(
        root,
        UiNode {
            grow: 1.0,
            ..UiNode::default()
        },
    );
    let l = t.layout([400.0, 300.0]);
    rect_is(&l, fixed, Rect::new(10.0, 10.0, 100.0, 280.0));
    rect_is(&l, grows, Rect::new(110.0, 10.0, 280.0, 280.0));
}

/// Columns 100 px, 1fr, 2fr with 10 px gaps in 400: the free 400 − 100 − 20 = 280 splits
/// 93.333 / 186.667, so columns start at 0, 110 and 213.333. Rows 40 and 60 with a 5 gap start
/// at 0 and 45. Five children fill row by row and stretch to their cells.
#[test]
fn grid_places_cells_in_explicit_tracks() {
    let mut t = Tree::new();
    let root = t.root(
        screen(400.0, 300.0),
        UiNode {
            gap: [10.0, 5.0],
            layout: Layout::Grid(Grid {
                columns: vec![Track::Px(100.0), Track::Fr(1.0), Track::Fr(2.0)],
                rows: vec![Track::Px(40.0), Track::Px(60.0)],
                ..Grid::default()
            }),
            ..UiNode::default()
        },
    );
    let kids: Vec<_> = (0..5).map(|_| t.child(root, UiNode::default())).collect();
    let l = t.layout([400.0, 300.0]);
    let third = 280.0 / 3.0;
    let want = [
        Rect::new(0.0, 0.0, 100.0, 40.0),
        Rect::new(110.0, 0.0, third, 40.0),
        Rect::new(120.0 + third, 0.0, 2.0 * third, 40.0),
        Rect::new(0.0, 45.0, 100.0, 60.0),
        Rect::new(110.0, 45.0, third, 60.0),
    ];
    for (id, r) in kids.iter().zip(want) {
        rect_is(&l, *id, r);
    }
}

/// A stack with padding 8 in 400 × 300: both children fill the content box (8, 8, 384 × 284).
#[test]
fn stack_overlaps_children_in_content_box() {
    let mut t = Tree::new();
    let root = t.root(
        screen(400.0, 300.0),
        UiNode {
            padding: Edges::all(8.0),
            layout: Layout::Stack,
            ..UiNode::default()
        },
    );
    let a = t.child(root, UiNode::default());
    let b = t.child(root, UiNode::default());
    let l = t.layout([400.0, 300.0]);
    rect_is(&l, a, Rect::new(8.0, 8.0, 384.0, 284.0));
    rect_is(&l, b, Rect::new(8.0, 8.0, 384.0, 284.0));
}

/// A TopRight preset (100 × 50, margin 8) in the 400 × 300 root: (400 − 108, 8) = (292, 8). Its
/// own child, in flow with padding 5, sits at (297, 13). Anchored nodes leave the parent's
/// flow: the in-flow sibling still starts at the root's corner.
#[test]
fn anchored_children_lay_out_as_islands() {
    let mut t = Tree::new();
    let root = t.root(screen(400.0, 300.0), UiNode::default());
    let sibling = t.child(root, sized(30.0, 30.0));
    let corner = t.child(
        root,
        UiNode {
            position: Position::Anchored(Anchored::preset(Preset::TopRight, [100.0, 50.0], 8.0)),
            padding: Edges::all(5.0),
            layout: flex(Direction::Row, Justify::Start, Align::Start),
            ..UiNode::default()
        },
    );
    let inner = t.child(corner, sized(20.0, 10.0));
    let l = t.layout([400.0, 300.0]);
    rect_is(&l, sibling, Rect::new(0.0, 0.0, 30.0, 30.0));
    rect_is(&l, corner, Rect::new(292.0, 8.0, 100.0, 50.0));
    rect_is(&l, inner, Rect::new(297.0, 13.0, 20.0, 10.0));
}

/// A mono "MMMM" at 20 px measures 4u × one line (u from the font tables); a 50 × 40 image
/// measures 50 × 40; a 10 × 8 region of it measures 10 × 8.
#[test]
fn text_and_image_nodes_measure_intrinsic_size_in_flex() {
    let m = metrics(&font_bytes("NotoSansMono-Regular.ttf"));
    let mut t = Tree::new();
    let root = t.root(
        screen(400.0, 300.0),
        UiNode {
            layout: flex(Direction::Row, Justify::Start, Align::Start),
            ..UiNode::default()
        },
    );
    let label = t.child(root, UiNode::default());
    t.world.insert(label, UiText::plain("MMMM")).unwrap();
    t.world
        .insert(
            label,
            UiStyle {
                font: Some(MONO.parse().unwrap()),
                font_size: Some(20.0),
                ..UiStyle::default()
            },
        )
        .unwrap();
    let image: concordia_assets::AssetRef = "builtin:texture/test_50x40".parse().unwrap();
    let pic = t.child(root, UiNode::default());
    t.world
        .insert(
            pic,
            UiImage {
                image: image.clone(),
                region: None,
                mode: ImageMode::Scale,
                tint: [1.0; 4],
                flip_h: false,
                flip_v: false,
            },
        )
        .unwrap();
    let cut = t.child(root, UiNode::default());
    t.world
        .insert(
            cut,
            UiImage {
                image: image.clone(),
                region: Some([0, 0, 10, 8]),
                mode: ImageMode::Scale,
                tint: [1.0; 4],
                flip_h: false,
                flip_v: false,
            },
        )
        .unwrap();
    let mut assets = MemoryAssets::default();
    assets.insert_image(&image, ImageData::solid(50, 40, [255; 4]));
    let l = t.layout_with([400.0, 300.0], &mut assets);
    let r = l.nodes[&label].rect;
    assert_close(r.w, 4.0 * m.mono_advance(20.0));
    assert_close(r.h, m.line_height(20.0));
    rect_is(&l, pic, Rect::new(r.w, 0.0, 50.0, 40.0));
    rect_is(&l, cut, Rect::new(r.w + 50.0, 0.0, 10.0, 8.0));
    assert_eq!(l.texts[&label].lines[0].text, "MMMM");
}

/// A Keep-stretched 800 × 400 canvas in 1600 × 900: rects stay in UI space (the full-rect child
/// is 800 × 400) and the root records the map (scale 2, offset (0, 50)).
#[test]
fn stretched_roots_lay_out_in_ui_space_and_record_their_map() {
    let mut t = Tree::new();
    let root = t.root(
        UiRoot {
            canvas: [800.0, 400.0],
            stretch: Stretch::Scale,
            ..UiRoot::default()
        },
        UiNode {
            layout: Layout::Stack,
            ..UiNode::default()
        },
    );
    let child = t.child(root, UiNode::default());
    let l = t.layout([1600.0, 900.0]);
    rect_is(&l, child, Rect::new(0.0, 0.0, 800.0, 400.0));
    assert_eq!(l.roots[0].entity, root);
    assert_eq!(
        l.roots[0].map,
        CanvasMap {
            scale: [2.0, 2.0],
            offset: [0.0, 50.0],
            size: [800.0, 400.0]
        }
    );
}

/// Siblings lay out by (`order`, id); hidden nodes and their children are skipped; invalid
/// components are reported and the node falls back to defaults.
#[test]
fn order_visibility_and_problems() {
    let mut t = Tree::new();
    let root = t.root(
        screen(400.0, 300.0),
        UiNode {
            layout: flex(Direction::Row, Justify::Start, Align::Start),
            ..UiNode::default()
        },
    );
    let first = t.child(
        root,
        UiNode {
            order: 1,
            ..sized(10.0, 10.0)
        },
    );
    let second = t.child(
        root,
        UiNode {
            order: 0,
            ..sized(20.0, 10.0)
        },
    );
    let hidden = t.child(
        root,
        UiNode {
            visible: false,
            ..sized(40.0, 10.0)
        },
    );
    let under_hidden = t.child(hidden, sized(5.0, 5.0));
    let bad = t.child(
        root,
        UiNode {
            width: Length::Px(-5.0),
            ..UiNode::default()
        },
    );
    let l = t.layout([400.0, 300.0]);
    rect_is(&l, second, Rect::new(0.0, 0.0, 20.0, 10.0));
    rect_is(&l, first, Rect::new(20.0, 0.0, 10.0, 10.0));
    assert!(!l.nodes.contains_key(&hidden) && !l.nodes.contains_key(&under_hidden));
    assert!(l.nodes.contains_key(&bad));
    assert_eq!(l.problems.len(), 1);
    assert_eq!(l.problems[0].0, bad);
    assert_eq!(l.problems[0].1.field_name(), Some("width"));
    // Draw order: the root, then children by (z, order, id): `second` and `bad` have order 0
    // (ids ascending), `first` order 1.
    assert_eq!(l.order, vec![root, second, bad, first]);
}
