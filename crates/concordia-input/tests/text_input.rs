//! Typed text as input (M2 phase 2.7b.3, ADR-0060, ADR-0072): `InputEvent::Text` events queue
//! like any event and come out of `sample` as the tick's `TypedText`, in order, and only on
//! their tick. Replays play them back like action changes. `ConsumedActions` can also hide every
//! key-bound action (a focused text box takes the keyboard).
//!
//! Checks: an `Insert` is 1..=4096 bytes with no control characters other than a newline or tab.

use std::collections::BTreeMap;

use concordia_input::{
    ActionDef, ActionKind, Binding, ConsumedActions, EditKey, InputEvent, InputMap, InputSampler,
    KeyCode, MouseButton, TextInput,
};

fn insert(s: &str) -> InputEvent {
    InputEvent::Text(TextInput::Insert(s.into()))
}

fn edit(key: EditKey, select: bool) -> TextInput {
    TextInput::Edit { key, select }
}

#[test]
fn typed_text_comes_out_on_its_tick_in_order() {
    let mut s = InputSampler::new(InputMap::starter());
    s.push(1, insert("he")).unwrap();
    s.push(1, InputEvent::Text(edit(EditKey::Backspace, false)))
        .unwrap();
    s.push(1, insert("llo")).unwrap();
    s.sample(0);
    assert!(s.typed().is_empty());
    s.sample(1);
    assert_eq!(
        s.typed(),
        [
            TextInput::Insert("he".into()),
            edit(EditKey::Backspace, false),
            TextInput::Insert("llo".into())
        ]
    );
    s.sample(2);
    assert!(s.typed().is_empty(), "text lasts one tick");
}

#[test]
fn inserts_must_be_short_printable_text() {
    let s = InputSampler::new(InputMap::starter());
    for ok in ["a", "two\nlines", "tab\there", "é✓"] {
        s.check(0, &insert(ok)).unwrap();
    }
    for bad in ["", "\u{7}", "nul\u{0}", &"x".repeat(4097)] {
        let err = s.check(0, &insert(bad)).unwrap_err();
        assert!(err.to_string().contains("text"), "{err}");
    }
}

#[test]
fn played_back_text_replaces_queued_text() {
    let mut s = InputSampler::new(InputMap::starter());
    s.push(0, insert("live")).unwrap();
    s.play_back(BTreeMap::new(), 2).unwrap();
    s.play_back_text(BTreeMap::from([(1, vec![TextInput::Insert("rec".into())])]));
    s.sample(0);
    assert!(
        s.typed().is_empty(),
        "queued text is dropped during playback"
    );
    s.sample(1);
    assert_eq!(s.typed(), [TextInput::Insert("rec".into())]);
}

#[test]
fn consumed_keys_hide_only_key_bound_actions() {
    let def = |kind, bindings| ActionDef { kind, bindings };
    let map = InputMap {
        actions: [
            (
                "jump",
                def(ActionKind::Button, vec![Binding::Key(KeyCode::Space)]),
            ),
            (
                "move",
                def(
                    ActionKind::Button,
                    vec![Binding::MouseButton(MouseButton::Left)],
                ),
            ),
            (
                "pan",
                def(
                    ActionKind::Axis1d,
                    vec![Binding::KeyAxis {
                        negative: KeyCode::A,
                        positive: KeyCode::D,
                    }],
                ),
            ),
        ]
        .into_iter()
        .map(|(n, d)| (n.to_owned(), d))
        .collect(),
    };
    let mut c = ConsumedActions::new(&map);
    c.consume_keys();
    assert!(c.keys_consumed());
    assert_eq!(
        ["jump", "move", "pan"].map(|a| c.is_consumed(a)),
        [true, false, true]
    );
    c.clear();
    assert!(!c.is_consumed("jump"));
}

#[test]
fn starter_map_has_selection_modifiers() {
    let map = InputMap::starter();
    assert_eq!(map.actions.len(), 18);
    assert_eq!(
        map.action("ui_shift").unwrap().bindings,
        [
            Binding::Key(KeyCode::ShiftLeft),
            Binding::Key(KeyCode::ShiftRight)
        ]
    );
    assert_eq!(
        map.action("ui_ctrl").unwrap().bindings,
        [
            Binding::Key(KeyCode::ControlLeft),
            Binding::Key(KeyCode::ControlRight),
            Binding::Key(KeyCode::SuperLeft),
            Binding::Key(KeyCode::SuperRight)
        ]
    );
}

#[test]
fn text_events_serialize_by_name() {
    let v = serde_json::to_value(InputEvent::Text(edit(EditKey::SelectAll, false))).unwrap();
    assert_eq!(
        v,
        serde_json::json!({"Text": {"Edit": {"key": "SelectAll", "select": false}}})
    );
    let v = serde_json::to_value(insert("hi")).unwrap();
    assert_eq!(v, serde_json::json!({"Text": {"Insert": "hi"}}));
}
