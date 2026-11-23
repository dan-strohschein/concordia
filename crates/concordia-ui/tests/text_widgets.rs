//! Text boxes, spin boxes and lists (M2 phase 2.7b.3): layout parts, and interaction through
//! the starter map and typed text. Widths come from shaping the same strings at the default
//! style (Noto Sans 16 px, line height 21.792), so `w(s)` below is that string's advance.
//!
//! - **Box** (10, 10)–(210, 40), no padding: single-line text is centered vertically, (30 −
//!   21.792)/2 = 4.104 down, so lines start at y = 14.104. With "hello", caret 5 and anchor 1,
//!   focused: the caret is (10 + w("hello"), 14.104, 1.5, 21.792) and the selection (10 + w("h"),
//!   14.104, w("hello") − w("h"), 21.792).
//! - **Scrolling:** a single-line box scrolls so the caret stays 2 px inside: in a 60-wide box
//!   whose text is wider, with the caret at the end, the scroll is w(text) + 2 − 60.
//! - **Password:** the drawn line is one "•" per character; the text stays "abc".
//! - **Spin box** (10, 10)–(110, 40): the arrows are a 20-wide column at the right, up (90, 10,
//!   20, 15) over down (90, 25, 20, 15), and the text's content is 80 wide.
//! - **Typing** goes to the focused box only, and while one has focus the keyboard is consumed.
//! - **Clicks:** a press puts the caret at the nearest character edge (x = 10 + w("he") + 1 →
//!   byte 2); a drag moves the caret, the anchor stays; Shift-click extends.
//! - **Spin box:** the up arrow adds the step (0 → 1); ↑ when focused adds another (2); typing
//!   "250" and Enter clamps to 100 and rewrites the text as "100".
//! - **List** (10, 10)–(210, 210), a column of five 40 px items: item i spans y 10 + 40i.. A click
//!   on item 1 selects [1]; in a multi list, Ctrl-click 3 gives [1, 3] and Shift-click 4 the range
//!   from the last plain or Ctrl click: [3, 4]. Focused, ↓ selects the item after the last
//!   clicked one (4 is the last, so [4]) and ↑ the one before ([3]). A single-select list
//!   ignores Ctrl.

mod common;

use common::play::{Play, at, click, key, mouse, tap};
use common::{Tree, assert_close};
use concordia_core::id::EntityId;
use concordia_input::ConsumedActions;
use concordia_ui::*;
use serde_json::{Value, json};

const LINE: f32 = 21.792;

fn w(s: &str) -> f32 {
    shape(
        &mut UiFonts::new(),
        &mut NoFonts,
        &UiText {
            wrap: Wrap::None,
            ..UiText::plain(s)
        },
        &ResolvedStyle::default(),
        None,
        None,
    )
    .width
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

fn boxed(t: &mut Tree, node: UiNode, b: UiTextBox, focused: bool) -> EntityId {
    let root = screen(t);
    let id = t.child(root, node);
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

#[track_caller]
fn rect_is(got: Rect, want: [f32; 4]) {
    for (a, b) in [got.x, got.y, got.w, got.h].into_iter().zip(want) {
        assert!((a - b).abs() < 1e-2, "{got:?} != {want:?}");
    }
}

fn parts(t: &Tree, id: EntityId) -> TextBoxParts {
    let l = t.layout([400.0, 300.0]);
    match l.parts.get(&id) {
        Some(Parts::TextBox(p)) => p.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn text_box_caret_and_selection_rects() {
    let mut t = Tree::new();
    let id = boxed(
        &mut t,
        pinned(10.0, 10.0, 210.0, 40.0),
        UiTextBox {
            text: "hello".into(),
            caret: 5,
            anchor: 1,
            ..UiTextBox::default()
        },
        true,
    );
    let p = parts(&t, id);
    let top = 10.0 + (30.0 - LINE) / 2.0;
    rect_is(p.caret.unwrap(), [10.0 + w("hello"), top, 1.5, LINE]);
    assert_eq!(p.selection.len(), 1);
    rect_is(
        p.selection[0],
        [10.0 + w("h"), top, w("hello") - w("h"), LINE],
    );
    assert_close(p.scroll, 0.0);
    assert!(!p.placeholder);
}

#[test]
fn unfocused_boxes_draw_no_caret_and_empty_ones_their_placeholder() {
    let mut t = Tree::new();
    let id = boxed(
        &mut t,
        pinned(10.0, 10.0, 210.0, 40.0),
        UiTextBox {
            placeholder: "Name".into(),
            ..UiTextBox::default()
        },
        false,
    );
    let p = parts(&t, id);
    assert!(p.caret.is_none() && p.placeholder);
    assert_eq!(p.text.lines[0].text, "Name");
}

#[test]
fn long_text_scrolls_to_keep_the_caret_in_view() {
    let text = "a long line of text";
    let mut t = Tree::new();
    let id = boxed(
        &mut t,
        pinned(10.0, 10.0, 70.0, 40.0),
        UiTextBox {
            text: text.into(),
            caret: text.len() as u32,
            anchor: text.len() as u32,
            ..UiTextBox::default()
        },
        true,
    );
    let p = parts(&t, id);
    assert_close(p.scroll, w(text) + 2.0 - 60.0);
    rect_is(
        p.caret.unwrap(),
        [10.0 + 60.0 - 2.0, 10.0 + (30.0 - LINE) / 2.0, 1.5, LINE],
    );
}

#[test]
fn password_masks_drawing_only() {
    let mut t = Tree::new();
    let id = boxed(
        &mut t,
        pinned(10.0, 10.0, 210.0, 40.0),
        UiTextBox {
            text: "abc".into(),
            password: true,
            ..UiTextBox::default()
        },
        false,
    );
    assert_eq!(parts(&t, id).text.lines[0].text, "•••");
    assert_eq!(t.world.get::<UiTextBox>(id).unwrap().text, "abc");
}

#[test]
fn spin_box_arrows_sit_at_the_right() {
    let mut t = Tree::new();
    let id = boxed(
        &mut t,
        pinned(10.0, 10.0, 110.0, 40.0),
        UiTextBox {
            text: "5".into(),
            ..UiTextBox::default()
        },
        false,
    );
    t.world.insert(id, UiSpinBox::default()).unwrap();
    let p = parts(&t, id);
    let (up, down) = p.spin.unwrap();
    rect_is(up, [90.0, 10.0, 20.0, 15.0]);
    rect_is(down, [90.0, 25.0, 20.0, 15.0]);
    let l = t.layout([400.0, 300.0]);
    assert_close(l.nodes[&id].content.w, 80.0);
}

fn text_of(play: &mut Play, id: &str) -> Value {
    play.get(id, "UiTextBox")
}

fn typed(play: &mut Play, events: Vec<Value>) {
    play.tick(events.into_iter().map(|e| json!({ "Text": e })).collect());
}

#[test]
fn typing_goes_to_the_focused_box_and_takes_the_keyboard() {
    let mut play = Play::new(1);
    let root = play.root();
    let a = play.pinned(&root, [10.0, 10.0, 210.0, 40.0], json!({"UiTextBox": {}}));
    let b = play.pinned(&root, [10.0, 60.0, 210.0, 90.0], json!({"UiTextBox": {}}));
    typed(&mut play, vec![json!({"Insert": "nobody"})]);
    assert_eq!(text_of(&mut play, &a)["text"], "");
    click(&mut play, 100.0, 25.0);
    play.tick(vec![json!({"Text": {"Insert": "hi"}}), key("Space", true)]);
    assert_eq!(text_of(&mut play, &a)["text"], "hi");
    assert_eq!(text_of(&mut play, &b)["text"], "");
    assert_eq!(
        play.signals::<UiTextChanged>(),
        [(a.clone(), UiTextChanged { text: "hi".into() })]
    );
    let consumed = play.resource::<ConsumedActions>().unwrap();
    assert!(
        consumed.keys_consumed(),
        "typing never reaches gameplay keys"
    );
    assert!(consumed.is_consumed("ui_accept"));
    play.tick(vec![key("Space", false)]);
    typed(
        &mut play,
        vec![json!({"Edit": {"key": "Enter", "select": false}})],
    );
    assert_eq!(
        play.signals::<UiTextSubmitted>(),
        [(a, UiTextSubmitted { text: "hi".into() })]
    );
}

#[test]
fn clicks_place_the_caret_and_drags_select() {
    let mut play = Play::new(1);
    let root = play.root();
    let a = play.pinned(
        &root,
        [10.0, 10.0, 210.0, 40.0],
        json!({"UiTextBox": {"text": "hello"}}),
    );
    let x = 10.0 + w("he") + 1.0;
    play.tick(vec![at(x, 25.0), mouse(true)]);
    let v = text_of(&mut play, &a);
    assert_eq!(
        (v["caret"].clone(), v["anchor"].clone()),
        (json!(2), json!(2))
    );
    play.tick(vec![at(10.0 + w("hello") + 5.0, 25.0)]);
    play.tick(vec![mouse(false)]);
    let v = text_of(&mut play, &a);
    assert_eq!(
        (v["caret"].clone(), v["anchor"].clone()),
        (json!(5), json!(2))
    );
    // Shift-click extends from the anchor.
    play.tick(vec![
        key("ShiftLeft", true),
        at(10.0 + w("h") - 1.0, 25.0),
        mouse(true),
    ]);
    play.tick(vec![mouse(false), key("ShiftLeft", false)]);
    let v = text_of(&mut play, &a);
    assert_eq!(
        (v["caret"].clone(), v["anchor"].clone()),
        (json!(1), json!(2))
    );
}

#[test]
fn copy_and_paste_go_through_the_engine_clipboard() {
    let mut play = Play::new(1);
    let root = play.root();
    play.pinned(
        &root,
        [10.0, 10.0, 210.0, 40.0],
        json!({"UiTextBox": {"text": "copy me"}}),
    );
    let b = play.pinned(&root, [10.0, 60.0, 210.0, 90.0], json!({"UiTextBox": {}}));
    let edit = |k: &str| json!({"Edit": {"key": k, "select": false}});
    click(&mut play, 100.0, 25.0);
    typed(&mut play, vec![edit("SelectAll"), edit("Copy")]);
    assert_eq!(play.resource::<UiClipboard>().unwrap().text, "copy me");
    click(&mut play, 100.0, 75.0);
    typed(&mut play, vec![edit("Paste")]);
    assert_eq!(text_of(&mut play, &b)["text"], "copy me");
    let r = play.ok("ui.clipboard", json!({}));
    assert_eq!(
        (r["text"].clone(), r["generation"].clone()),
        (json!("copy me"), json!(1))
    );
}

#[test]
fn spin_box_steps_types_and_clamps() {
    let mut play = Play::new(1);
    let root = play.root();
    let s = play.pinned(
        &root,
        [10.0, 10.0, 110.0, 40.0],
        json!({"UiTextBox": {}, "UiSpinBox": {}}),
    );
    play.tick(vec![]);
    assert_eq!(
        text_of(&mut play, &s)["text"],
        "0",
        "the text shows the value"
    );
    click(&mut play, 100.0, 15.0);
    assert_eq!(play.get(&s, "UiSpinBox")["value"], 1.0);
    tap(&mut play, "ArrowUp");
    assert_eq!(play.get(&s, "UiSpinBox")["value"], 2.0);
    let edit = |k: &str| json!({"Text": {"Edit": {"key": k, "select": false}}});
    play.tick(vec![edit("SelectAll"), json!({"Text": {"Insert": "250"}})]);
    assert_eq!(
        text_of(&mut play, &s)["text"],
        "250",
        "typing isn't overwritten"
    );
    play.tick(vec![edit("Enter")]);
    assert_eq!(play.get(&s, "UiSpinBox")["value"], 100.0);
    assert_eq!(text_of(&mut play, &s)["text"], "100");
    let values: Vec<f32> = play
        .signals::<UiValueChanged>()
        .into_iter()
        .map(|(_, v)| v.value)
        .collect();
    assert_eq!(values, [100.0]);
}

fn list(play: &mut Play, multi: bool) -> String {
    let root = play.root();
    let l = play.spawn(json!({
        "Parent": root,
        "UiList": {"multi": multi},
        "UiNode": {"layout": {"Flex": {"direction": "Column"}}, "position": {"Anchored": {"offsets": {"left": 10.0, "top": 10.0, "right": 210.0, "bottom": 210.0}}}},
    }));
    for _ in 0..5 {
        play.spawn(json!({"Parent": l, "UiNode": {"height": {"Px": 40.0}}}));
    }
    l
}

fn selected(play: &mut Play, l: &str) -> Value {
    play.get(l, "UiList")["selected"].clone()
}

#[test]
fn list_selects_single_ctrl_and_shift_ranges() {
    let mut play = Play::new(1);
    let l = list(&mut play, true);
    let item = |i: f32| 10.0 + 40.0 * i + 20.0;
    click(&mut play, 100.0, item(1.0));
    assert_eq!(selected(&mut play, &l), json!([1]));
    play.tick(vec![
        key("ControlLeft", true),
        at(100.0, item(3.0)),
        mouse(true),
    ]);
    play.tick(vec![mouse(false), key("ControlLeft", false)]);
    assert_eq!(selected(&mut play, &l), json!([1, 3]));
    play.tick(vec![
        key("ShiftLeft", true),
        at(100.0, item(4.0)),
        mouse(true),
    ]);
    assert_eq!(
        play.signals::<UiSelected>(),
        [(l.clone(), UiSelected { index: 4 })]
    );
    play.tick(vec![mouse(false), key("ShiftLeft", false)]);
    assert_eq!(selected(&mut play, &l), json!([3, 4]));
    tap(&mut play, "ArrowDown");
    assert_eq!(selected(&mut play, &l), json!([4]), "the last item stays");
    tap(&mut play, "ArrowUp");
    assert_eq!(selected(&mut play, &l), json!([3]));
    // A single-select list ignores Ctrl.
    let mut play = Play::new(1);
    let one = list(&mut play, false);
    click(&mut play, 100.0, item(0.0));
    play.tick(vec![
        key("ControlLeft", true),
        at(100.0, item(2.0)),
        mouse(true),
    ]);
    play.tick(vec![mouse(false), key("ControlLeft", false)]);
    assert_eq!(selected(&mut play, &one), json!([2]));
}
