//! `ui.interact` (2.7b.2): the pointer, wheel and focus keys reach widgets through the starter
//! map's `ui_*` actions, inside the simulation. Every expectation is derived here.
//!
//! The root is 400 × 300 with stretch disabled; with no `Viewport` input the viewport is the
//! first root's canvas, so pointer coordinates are UI coordinates. A tick's events apply at that
//! tick; a signal raised in a tick is pending once the step returns.
//!
//! - **Buttons** (WAI-ARIA): press over a widget marks it `pressed` (and focuses it); release
//!   over the same widget fires `UiPressed`; release anywhere clears `pressed`. A press that
//!   starts elsewhere never fires. A disabled widget is never hovered, pressed or focused.
//! - **Hover:** the topmost widget under the pointer is `hovered`, and `hover_ticks` counts the
//!   ticks it has been.
//! - **Toggles** flip on release like buttons and raise `UiToggled`. In a group, one turning on
//!   turns the others off (each raises its signal); clicking the one that's on leaves it on
//!   unless it allows none.
//! - **Slider** (100, 100)–(300, 120), 0..10, step 1, page 3: the track is x 108..292 (184 wide),
//!   so a press at x = 200 is fraction 92/184 = 0.5 → 5, and a drag to x = 260 is 152/184 =
//!   0.826 → 8.26 → 8. Focused, → is +1 (9, then 10, then 10 with no signal), Page Down −3 (7),
//!   ← −1 (6).
//! - **Dropdown** (100, 50)–(250, 80) over Low / Medium / High: a press on the face opens it; rows
//!   are 21.792 + 8 = 29.792 tall from y = 80, so row 2 spans 139.584..169.376 and a press at
//!   y = 150 picks it. A press outside closes it unchanged. Focused: Enter opens, ↑ / ↓ move the
//!   highlight (clamped), Enter picks it, Escape closes.
//! - **Tabs** over the whole root: titles are their text width + 24 from x = 0, so x = w("One") +
//!   24 + 5 is on "Two". ← and → move the selection when focused.
//! - **Tooltip** delay 3: hovered ticks 1, 2 show nothing; at tick 3 the tip appears where the
//!   pointer is; leaving clears it.
//! - **Scroll** (0, 0)–(200, 100) over five 50 px rows (content 250, offset 0..150). A wheel line
//!   is 40 px and positive dy scrolls up: dy = −2 gives 80; dy = 10 clamps to 0. At 80 the thumb
//!   (40 tall) is at 80/150 · 60 = 32; pressing it at y = 40 grabs it 8 below its top, and moving
//!   to y = 70 puts its top at 62 → 62/60 · 150 = 155 → 150.
//! - **Focus:** Tab goes through widgets with a `UiFocus.order` first (ascending), then tree
//!   order, wrapping; Shift-Tab goes back. Arrows take an explicit neighbour, else the nearest
//!   widget whose center lies that way, scored by distance along + 2 × distance across.
//!   Centers: A (20, 20), B (120, 20), C (80, 120), D (120, 170). From A, → : B (100 + 0) beats
//!   C (60 + 200) and D (100 + 300); from B, ↓ : D (150 + 0) beats C (100 + 2·40 = 180) (with a
//!   weight of 1, C's 140 would win); from D, ↑ : C (50 + 80 = 130) beats B (150); from C, ↑ : B
//!   (100 + 80 = 180) beats A (100 + 120 = 220); from B, ← : A; from A, ↓ with `down: D` set: D
//!   (nearest would be C, 100 + 120 = 220 against D's 150 + 200 = 350).
//! - **Accept** (Enter or Space) presses the focused button.
//! - **Viewport:** a button anchored to the top right (anchors 1, 0, 1, 0; offsets −100, 10,
//!   −10, 40) is at x 300..390 in the default 400-wide viewport and at 700..790 once a
//!   `Viewport` of 800 × 600 arrives (stretch disabled: UI space is the viewport).
//! - **Consumption:** the pointer is consumed over an interactive widget, a box with a background
//!   or a `Stop` node, and while a press is held; not over empty space or an `Ignore` node.

mod common;

use common::play::{Play, at, click, key, mouse, tap, viewport, wheel};
use common::{font_bytes, metrics};
use concordia_audio::PlayCue;
use concordia_input::ConsumedActions;
use concordia_ui::*;
use serde_json::{Value, json};

fn state(play: &mut Play, id: &str) -> UiState {
    serde_json::from_value(play.get(id, "UiState")).unwrap_or_default()
}

fn count<S: concordia_terra::Component + Clone>(play: &mut Play, id: &str) -> usize {
    play.signals::<S>().iter().filter(|(e, _)| e == id).count()
}

fn button(play: &mut Play, root: &str, ltrb: [f32; 4]) -> String {
    play.pinned(root, ltrb, json!({"UiButton": {}}))
}

#[test]
fn button_fires_on_release_inside_only() {
    let mut play = Play::new(1);
    let root = play.root();
    let b = button(&mut play, &root, [100.0, 100.0, 200.0, 140.0]);
    play.tick(vec![at(150.0, 120.0), mouse(true)]);
    let s = state(&mut play, &b);
    assert!(s.hovered && s.pressed && s.focused, "{s:?}");
    assert_eq!(count::<UiPressed>(&mut play, &b), 0);
    play.tick(vec![mouse(false)]);
    assert_eq!(count::<UiPressed>(&mut play, &b), 1);
    assert!(!state(&mut play, &b).pressed);
    play.tick(vec![]);
    assert_eq!(count::<UiPressed>(&mut play, &b), 0, "fires once");
}

#[test]
fn button_press_then_drag_off_does_not_fire() {
    let mut play = Play::new(1);
    let root = play.root();
    let b = button(&mut play, &root, [100.0, 100.0, 200.0, 140.0]);
    play.tick(vec![at(150.0, 120.0), mouse(true)]);
    play.tick(vec![at(300.0, 250.0)]);
    play.tick(vec![mouse(false)]);
    assert_eq!(count::<UiPressed>(&mut play, &b), 0);
    assert!(!state(&mut play, &b).pressed);
    // A press that starts outside doesn't fire on release inside.
    play.tick(vec![mouse(true)]);
    play.tick(vec![at(150.0, 120.0)]);
    play.tick(vec![mouse(false)]);
    assert_eq!(count::<UiPressed>(&mut play, &b), 0);
}

#[test]
fn disabled_button_ignores_input() {
    let mut play = Play::new(1);
    let root = play.root();
    let b = play.pinned(
        &root,
        [100.0, 100.0, 200.0, 140.0],
        json!({"UiButton": {}, "UiState": {"disabled": true}}),
    );
    play.tick(vec![at(150.0, 120.0), mouse(true)]);
    let s = state(&mut play, &b);
    assert!(
        s.disabled && !s.hovered && !s.pressed && !s.focused,
        "{s:?}"
    );
    play.tick(vec![mouse(false)]);
    assert_eq!(count::<UiPressed>(&mut play, &b), 0);
    tap(&mut play, "Tab");
    assert!(!state(&mut play, &b).focused, "not focusable");
}

#[test]
fn hover_and_press_state_follow_the_pointer() {
    let mut play = Play::new(1);
    let root = play.root();
    let b = button(&mut play, &root, [100.0, 100.0, 200.0, 140.0]);
    play.tick(vec![at(150.0, 120.0)]);
    let s = state(&mut play, &b);
    assert!(s.hovered && !s.pressed && s.hover_ticks == 1, "{s:?}");
    play.tick(vec![]);
    play.tick(vec![]);
    assert_eq!(state(&mut play, &b).hover_ticks, 3);
    play.tick(vec![at(10.0, 10.0)]);
    let s = state(&mut play, &b);
    assert!(!s.hovered && s.hover_ticks == 0, "{s:?}");
}

fn toggle(play: &mut Play, root: &str, y: f32, value: Value) -> String {
    play.pinned(
        root,
        [100.0, y, 200.0, y + 24.0],
        json!({ "UiToggle": value }),
    )
}

fn on(play: &mut Play, id: &str) -> bool {
    play.get(id, "UiToggle")["on"].as_bool().unwrap()
}

#[test]
fn toggle_flips_and_signals() {
    let mut play = Play::new(1);
    let root = play.root();
    let t = toggle(&mut play, &root, 100.0, json!({}));
    click(&mut play, 150.0, 110.0);
    assert!(on(&mut play, &t));
    assert_eq!(
        play.signals::<UiToggled>(),
        [(t.clone(), UiToggled { on: true })]
    );
    click(&mut play, 150.0, 110.0);
    assert!(!on(&mut play, &t));
    assert_eq!(play.signals::<UiToggled>(), [(t, UiToggled { on: false })]);
}

#[test]
fn radio_group_keeps_exactly_one_on() {
    let mut play = Play::new(1);
    let root = play.root();
    let a = toggle(&mut play, &root, 100.0, json!({"on": true, "group": "g"}));
    let b = toggle(&mut play, &root, 130.0, json!({"group": "g"}));
    let c = toggle(&mut play, &root, 160.0, json!({"group": "g"}));
    click(&mut play, 150.0, 140.0);
    assert_eq!(
        [on(&mut play, &a), on(&mut play, &b), on(&mut play, &c)],
        [false, true, false]
    );
    let mut signals = play.signals::<UiToggled>();
    signals.sort_by(|x, y| x.0.cmp(&y.0));
    let mut want = vec![
        (a.clone(), UiToggled { on: false }),
        (b.clone(), UiToggled { on: true }),
    ];
    want.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(signals, want);
    click(&mut play, 150.0, 140.0);
    assert!(on(&mut play, &b), "a radio stays on");
    assert!(play.signals::<UiToggled>().is_empty());
    // A group that allows none lets the one that's on turn off.
    play.ok(
        "component.patch",
        json!({"entity": b, "component": "UiToggle", "patch": {"allow_none": true}, "world": "play"}),
    );
    click(&mut play, 150.0, 140.0);
    assert!(!on(&mut play, &b));
}

fn value(play: &mut Play, id: &str) -> f64 {
    play.get(id, "UiSlider")["value"].as_f64().unwrap()
}

fn changes(play: &mut Play) -> Vec<f32> {
    play.signals::<UiValueChanged>()
        .into_iter()
        .map(|(_, v)| v.value)
        .collect()
}

#[test]
fn slider_drag_and_keys_snap_to_step() {
    let mut play = Play::new(1);
    let root = play.root();
    let s = play.pinned(
        &root,
        [100.0, 100.0, 300.0, 120.0],
        json!({"UiSlider": {"max": 10.0, "step": 1.0, "page": 3.0}}),
    );
    play.tick(vec![at(200.0, 110.0), mouse(true)]);
    assert_eq!(value(&mut play, &s), 5.0);
    assert_eq!(changes(&mut play), [5.0]);
    play.tick(vec![at(260.0, 110.0)]);
    assert_eq!(value(&mut play, &s), 8.0);
    play.tick(vec![mouse(false)]);
    for (k, want, signalled) in [
        ("ArrowRight", 9.0, true),
        ("ArrowRight", 10.0, true),
        ("ArrowRight", 10.0, false),
        ("PageDown", 7.0, true),
        ("ArrowLeft", 6.0, true),
    ] {
        play.tick(vec![key(k, true)]);
        assert_eq!(value(&mut play, &s), want, "{k}");
        assert_eq!(changes(&mut play).len(), usize::from(signalled), "{k}");
        play.tick(vec![key(k, false)]);
    }
}

fn dropdown(play: &mut Play) -> (String, String) {
    let root = play.root();
    let d = play.pinned(
        &root,
        [100.0, 50.0, 250.0, 80.0],
        json!({"UiDropdown": {"options": ["Low", "Medium", "High"], "selected": 0}}),
    );
    (root, d)
}

fn dd(play: &mut Play, id: &str) -> (bool, Value) {
    let v = play.get(id, "UiDropdown");
    (v["open"].as_bool().unwrap(), v["selected"].clone())
}

#[test]
fn dropdown_opens_selects_and_closes() {
    let mut play = Play::new(1);
    let (_, d) = dropdown(&mut play);
    click(&mut play, 175.0, 65.0);
    assert_eq!(dd(&mut play, &d), (true, json!(0)));
    play.tick(vec![at(150.0, 150.0), mouse(true)]);
    assert_eq!(dd(&mut play, &d), (false, json!(2)));
    assert_eq!(
        play.signals::<UiSelected>(),
        [(d.clone(), UiSelected { index: 2 })]
    );
    play.tick(vec![mouse(false)]);
    click(&mut play, 175.0, 65.0);
    assert!(dd(&mut play, &d).0);
    click(&mut play, 10.0, 290.0);
    assert_eq!(dd(&mut play, &d), (false, json!(2)), "outside closes");
    assert!(play.signals::<UiSelected>().is_empty());
    // Keys: Enter opens, ↓ clamps at the last, ↑ moves up, Enter picks.
    tap(&mut play, "Enter");
    assert!(dd(&mut play, &d).0);
    tap(&mut play, "ArrowDown");
    tap(&mut play, "ArrowUp");
    assert_eq!(state(&mut play, &d).item, Some(1));
    play.tick(vec![key("Enter", true)]);
    assert_eq!(dd(&mut play, &d), (false, json!(1)));
    assert_eq!(
        play.signals::<UiSelected>(),
        [(d.clone(), UiSelected { index: 1 })]
    );
    play.tick(vec![key("Enter", false)]);
    tap(&mut play, "Enter");
    tap(&mut play, "Escape");
    assert_eq!(dd(&mut play, &d), (false, json!(1)));
}

#[test]
fn tabs_select_by_click_and_arrows() {
    let mut play = Play::new(1);
    let root = play.root();
    let tabs = play.pinned(
        &root,
        [0.0, 0.0, 400.0, 300.0],
        json!({"UiTabs": {"titles": ["One", "Two"]}, "UiNode": {"layout": "Stack", "position": {"Anchored": {"anchors": {"left": 0.0, "top": 0.0, "right": 1.0, "bottom": 1.0}}}}}),
    );
    for _ in 0..2 {
        play.spawn(json!({"Parent": tabs, "UiNode": {}}));
    }
    let style = ResolvedStyle::default();
    let one = shape(
        &mut UiFonts::new(),
        &mut NoFonts,
        &UiText::plain("One"),
        &style,
        None,
        None,
    );
    click(&mut play, one.width + 24.0 + 5.0, 10.0);
    assert_eq!(play.get(&tabs, "UiTabs")["selected"], 1);
    tap(&mut play, "ArrowLeft");
    assert_eq!(play.get(&tabs, "UiTabs")["selected"], 0);
    assert_eq!(
        play.signals::<UiSelected>(),
        Vec::<(String, UiSelected)>::new(),
        "signals were last tick's"
    );
}

#[test]
fn tooltip_shows_after_delay_ticks() {
    let mut play = Play::new(1);
    let root = play.root();
    let b = play.pinned(
        &root,
        [100.0, 100.0, 200.0, 140.0],
        json!({"UiButton": {}, "UiTooltip": {"text": "Saves", "delay_ticks": 3}}),
    );
    play.tick(vec![at(150.0, 120.0)]);
    play.tick(vec![]);
    assert_eq!(state(&mut play, &b).tip, None);
    play.tick(vec![]);
    assert_eq!(state(&mut play, &b).tip, Some([150.0, 120.0]));
    play.tick(vec![at(10.0, 10.0)]);
    assert_eq!(state(&mut play, &b).tip, None);
}

#[test]
fn wheel_and_bar_drag_scroll() {
    let mut play = Play::new(1);
    let root = play.root();
    let s = play.pinned(
        &root,
        [0.0, 0.0, 200.0, 100.0],
        json!({"UiScroll": {}, "UiNode": {"layout": {"Flex": {"direction": "Column"}}, "position": {"Anchored": {"offsets": {"left": 0.0, "top": 0.0, "right": 200.0, "bottom": 100.0}}}}}),
    );
    for _ in 0..5 {
        play.spawn(json!({"Parent": s, "UiNode": {"height": {"Px": 50.0}}}));
    }
    let offset = |play: &mut Play| play.get(&s, "UiScroll")["offset"][1].as_f64().unwrap();
    play.tick(vec![at(100.0, 50.0), wheel(-2.0)]);
    assert_eq!(offset(&mut play), 80.0);
    play.tick(vec![wheel(10.0)]);
    assert_eq!(offset(&mut play), 0.0);
    play.tick(vec![wheel(-2.0)]);
    play.tick(vec![at(196.0, 40.0), mouse(true)]);
    assert_eq!(
        state(&mut play, &s).grab,
        Some(Grab {
            bar: Orientation::Vertical,
            at: 8.0
        })
    );
    play.tick(vec![at(196.0, 70.0)]);
    assert_eq!(offset(&mut play), 150.0);
    play.tick(vec![mouse(false)]);
    assert_eq!(state(&mut play, &s).grab, None);
}

fn focused(play: &mut Play, ids: &[&String]) -> Vec<bool> {
    ids.iter().map(|id| state(play, id).focused).collect()
}

#[test]
fn focus_moves_by_tab_order_and_tree_order() {
    let mut play = Play::new(1);
    let root = play.root();
    let a = button(&mut play, &root, [0.0, 0.0, 40.0, 40.0]);
    let b = button(&mut play, &root, [100.0, 0.0, 140.0, 40.0]);
    let c = play.pinned(
        &root,
        [0.0, 100.0, 40.0, 140.0],
        json!({"UiButton": {}, "UiFocus": {"order": 0}}),
    );
    let ids = [&a, &b, &c];
    tap(&mut play, "Tab");
    assert_eq!(focused(&mut play, &ids), [false, false, true]);
    play.tick(vec![key("Tab", true)]);
    assert_eq!(focused(&mut play, &ids), [true, false, false]);
    let mut moved = play.signals::<UiFocusChanged>();
    moved.sort_by(|x, y| x.0.cmp(&y.0));
    let mut want = vec![
        (c.clone(), UiFocusChanged { focused: false }),
        (a.clone(), UiFocusChanged { focused: true }),
    ];
    want.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(moved, want);
    play.tick(vec![key("Tab", false)]);
    tap(&mut play, "Tab");
    assert_eq!(focused(&mut play, &ids), [false, true, false]);
    tap(&mut play, "Tab");
    assert_eq!(focused(&mut play, &ids), [false, false, true], "wraps");
    play.tick(vec![key("ShiftLeft", true)]);
    tap(&mut play, "Tab");
    play.tick(vec![key("ShiftLeft", false)]);
    assert_eq!(
        focused(&mut play, &ids),
        [false, true, false],
        "Shift-Tab goes back"
    );
}

#[test]
fn focus_moves_by_arrows_to_nearest_or_explicit_neighbour() {
    let mut play = Play::new(1);
    let root = play.root();
    let a = button(&mut play, &root, [0.0, 0.0, 40.0, 40.0]);
    let b = button(&mut play, &root, [100.0, 0.0, 140.0, 40.0]);
    let c = button(&mut play, &root, [60.0, 100.0, 100.0, 140.0]);
    let d = button(&mut play, &root, [100.0, 150.0, 140.0, 190.0]);
    play.ok(
        "component.set",
        json!({"entity": a, "component": "UiFocus", "value": {"down": d}}),
    );
    let ids = [&a, &b, &c, &d];
    tap(&mut play, "Tab");
    assert_eq!(focused(&mut play, &ids), [true, false, false, false]);
    for (k, want) in [
        ("ArrowRight", [false, true, false, false]),
        ("ArrowDown", [false, false, false, true]),
        ("ArrowUp", [false, false, true, false]),
        ("ArrowUp", [false, true, false, false]),
        ("ArrowLeft", [true, false, false, false]),
        ("ArrowDown", [false, false, false, true]),
    ] {
        tap(&mut play, k);
        assert_eq!(focused(&mut play, &ids), want, "{k}");
    }
}

#[test]
fn accept_presses_the_focused_button() {
    let mut play = Play::new(1);
    let root = play.root();
    let b = button(&mut play, &root, [100.0, 100.0, 200.0, 140.0]);
    tap(&mut play, "Tab");
    for k in ["Enter", "Space"] {
        play.tick(vec![key(k, true)]);
        assert_eq!(count::<UiPressed>(&mut play, &b), 1, "{k}");
        play.tick(vec![key(k, false)]);
    }
}

#[test]
fn ui_sounds_raise_play_cue_on_the_widget() {
    let mut play = Play::new(1);
    let root = play.root();
    let b = play.pinned(
        &root,
        [100.0, 100.0, 200.0, 140.0],
        json!({"UiButton": {}, "UiSounds": {
            "hover": "builtin:cue/hover", "press": "builtin:cue/press", "release": "builtin:cue/release"
        }}),
    );
    let cues = |play: &mut Play| -> Vec<(String, String)> {
        play.signals::<PlayCue>()
            .into_iter()
            .map(|(e, c)| (e, c.cue.to_string()))
            .collect()
    };
    play.tick(vec![at(150.0, 120.0)]);
    assert_eq!(cues(&mut play), [(b.clone(), "builtin:cue/hover".into())]);
    play.tick(vec![]);
    assert!(cues(&mut play).is_empty(), "hover sounds once");
    play.tick(vec![mouse(true)]);
    assert_eq!(cues(&mut play), [(b.clone(), "builtin:cue/press".into())]);
    play.tick(vec![mouse(false)]);
    assert_eq!(cues(&mut play), [(b, "builtin:cue/release".into())]);
}

#[test]
fn viewport_input_reflows_layout() {
    let mut play = Play::new(1);
    let root = play.root();
    let b = play.spawn(json!({
        "Parent": root,
        "UiButton": {},
        "UiNode": {"position": {"Anchored": {
            "anchors": {"left": 1.0, "top": 0.0, "right": 1.0, "bottom": 0.0},
            "offsets": {"left": -100.0, "top": 10.0, "right": -10.0, "bottom": 40.0}
        }}},
    }));
    click(&mut play, 745.0, 25.0);
    assert_eq!(
        count::<UiPressed>(&mut play, &b),
        0,
        "off the 400-wide viewport"
    );
    click(&mut play, 345.0, 25.0);
    assert_eq!(count::<UiPressed>(&mut play, &b), 1);
    play.tick(vec![viewport(800.0, 600.0)]);
    click(&mut play, 745.0, 25.0);
    assert_eq!(count::<UiPressed>(&mut play, &b), 1, "reflowed to 700..790");
}

#[test]
fn pointer_over_ui_is_consumed_and_elsewhere_is_not() {
    let mut play = Play::new(1);
    let root = play.root();
    button(&mut play, &root, [100.0, 100.0, 200.0, 140.0]);
    play.pinned(
        &root,
        [0.0, 200.0, 100.0, 300.0],
        json!({"UiStyle": {"background": {"Solid": [0.1, 0.1, 0.1, 1.0]}}}),
    );
    play.pinned(
        &root,
        [300.0, 200.0, 400.0, 300.0],
        json!({"UiStyle": {"background": {"Solid": [0.1, 0.1, 0.1, 1.0]}}, "UiNode": {
            "pointer": "Ignore",
            "position": {"Anchored": {"offsets": {"left": 300.0, "top": 200.0, "right": 400.0, "bottom": 300.0}}}
        }}),
    );
    play.pinned(
        &root,
        [200.0, 0.0, 300.0, 50.0],
        json!({"UiNode": {"pointer": "Stop", "position": {"Anchored": {"offsets": {"left": 200.0, "top": 0.0, "right": 300.0, "bottom": 50.0}}}}}),
    );
    for (x, y, want) in [
        (150.0, 120.0, true),
        (50.0, 250.0, true),
        (350.0, 250.0, false),
        (250.0, 25.0, true),
        (10.0, 10.0, false),
    ] {
        play.tick(vec![at(x, y)]);
        let consumed = play.resource::<ConsumedActions>().unwrap();
        assert_eq!(consumed.pointer_consumed(), want, "({x}, {y})");
        assert_eq!(consumed.is_consumed("move"), want, "({x}, {y})");
    }
    // A held press keeps the pointer even off the UI.
    play.tick(vec![at(150.0, 120.0), mouse(true)]);
    play.tick(vec![at(10.0, 10.0)]);
    assert!(
        play.resource::<ConsumedActions>()
            .unwrap()
            .pointer_consumed()
    );
    play.tick(vec![mouse(false)]);
    play.tick(vec![]);
    assert!(
        !play
            .resource::<ConsumedActions>()
            .unwrap()
            .pointer_consumed()
    );
}

/// The line height the dropdown rows use, read from the font (a sanity check of the header).
#[test]
fn dropdown_row_height_comes_from_the_font() {
    let m = metrics(&font_bytes("NotoSans-Regular.ttf"));
    assert!((m.line_height(16.0) + 8.0 - 29.792).abs() < 1e-3);
}

/// `ui.hit_test`: a panel (background) at (50, 50)–(350, 250) holds a button anchored at
/// (50, 50)–(150, 90) inside it, so at (100, 100)–(200, 140); its style asks for the pointer
/// cursor. (150, 120) hits the button: path root → panel → button, part `Body`, cursor
/// `Pointer`, UI point (150, 120). (60, 60) hits the panel, which is no widget: cursor `Arrow`.
/// (10, 10) hits nothing. In the play world the viewport is the input's; a 0 width is refused.
#[test]
fn ui_hit_test_returns_topmost_widget_path() {
    let mut play = Play::new(1);
    let root = play.root();
    let panel = play.pinned(
        &root,
        [50.0, 50.0, 350.0, 250.0],
        json!({"UiStyle": {"background": {"Solid": [0.1, 0.1, 0.1, 1.0]}}}),
    );
    let b = play.pinned(
        &panel,
        [50.0, 50.0, 150.0, 90.0],
        json!({"UiButton": {}, "UiStyle": {"cursor": "Pointer"}}),
    );
    let r = play.ok("ui.hit_test", json!({"x": 150.0, "y": 120.0}));
    assert_eq!(r["node"], json!(b));
    assert_eq!(r["widget"], json!(b));
    assert_eq!(r["path"], json!([root, panel, b]));
    assert_eq!(r["part"], json!("Body"));
    assert_eq!(r["cursor"], json!("Pointer"));
    assert_eq!(r["point"], json!([150.0, 120.0]));
    let r = play.ok("ui.hit_test", json!({"x": 60.0, "y": 60.0}));
    assert_eq!(r["node"], json!(panel));
    assert_eq!(r["widget"], Value::Null);
    assert_eq!(r["cursor"], json!("Arrow"));
    let r = play.ok("ui.hit_test", json!({"x": 10.0, "y": 10.0}));
    assert_eq!(r["node"], Value::Null);
    assert_eq!(r["path"], json!([]));
    // The play world uses the input's viewport: at 800 × 600 nothing moves (all offsets are
    // from the top left), and `ui.inspect` reports the hover state.
    play.tick(vec![viewport(800.0, 600.0), at(150.0, 120.0)]);
    let r = play.ok(
        "ui.hit_test",
        json!({"x": 150.0, "y": 120.0, "world": "play"}),
    );
    assert_eq!(r["viewport"], json!([800.0, 600.0]));
    let inspect = play.ok("ui.inspect", json!({"world": "play", "entity": b}));
    assert_eq!(inspect["viewport"], json!([800.0, 600.0]));
    assert_eq!(inspect["nodes"][0]["state"]["hovered"], true);
    let err = play
        .call(
            "ui.hit_test",
            json!({"x": 1.0, "y": 1.0, "width": 0, "height": 10}),
        )
        .unwrap_err();
    assert_eq!(err.kind, concordia_api::ApiErrorKind::InvalidParams);
}
