//! Input the in-game UI needs (M2 phase 2.7b.2, ADR-0060): the viewport as an action, pointer
//! consumption, and the starter map's `ui_*` actions.
//!
//! - The viewport (logical pixels) reaches the simulation only through an action bound to
//!   `Binding::Viewport`, so replays record it like any action (ADR-0072 decision 2). Before any
//!   `Viewport` event it is (0, 0); it holds until the next event.
//! - `ConsumedActions` lists every action with a pointer binding (mouse button, pointer position,
//!   ray, motion, scroll). Consuming the pointer hides exactly those from `unconsumed`; keys stay.

use concordia_core::math::Vec2;
use concordia_input::{
    ActionDef, ActionKind, ActionValue, Binding, ConsumedActions, InputEvent, InputMap,
    InputSampler, KeyCode, MouseButton,
};

fn map() -> InputMap {
    let def = |kind, bindings| ActionDef { kind, bindings };
    let map = InputMap {
        actions: [
            ("view", def(ActionKind::Axis2d, vec![Binding::Viewport])),
            (
                "move",
                def(
                    ActionKind::Button,
                    vec![Binding::MouseButton(MouseButton::Left)],
                ),
            ),
            ("aim", def(ActionKind::Ray, vec![Binding::PointerRay])),
            ("zoom", def(ActionKind::Axis1d, vec![Binding::Scroll])),
            (
                "jump",
                def(ActionKind::Button, vec![Binding::Key(KeyCode::Space)]),
            ),
            (
                "fire",
                def(
                    ActionKind::Button,
                    vec![
                        Binding::Key(KeyCode::F),
                        Binding::MouseButton(MouseButton::Right),
                    ],
                ),
            ),
        ]
        .into_iter()
        .map(|(n, d)| (n.to_owned(), d))
        .collect(),
    };
    map.validate().unwrap();
    map
}

fn axis(s: &mut InputSampler, tick: u64, name: &str) -> Vec2 {
    match s.sample(tick).get(name).unwrap().value {
        ActionValue::Axis2d(v) => v,
        other => panic!("{name} is {other:?}"),
    }
}

#[test]
fn viewport_event_sets_the_viewport_binding_until_the_next() {
    let mut s = InputSampler::new(map());
    assert_eq!(axis(&mut s, 0, "view"), Vec2::ZERO);
    s.push(
        1,
        InputEvent::Viewport {
            width: 1600.0,
            height: 900.0,
        },
    )
    .unwrap();
    assert_eq!(axis(&mut s, 1, "view"), Vec2::new(1600.0, 900.0));
    assert_eq!(axis(&mut s, 2, "view"), Vec2::new(1600.0, 900.0));
    s.push(
        3,
        InputEvent::Viewport {
            width: 800.0,
            height: 600.0,
        },
    )
    .unwrap();
    assert_eq!(axis(&mut s, 3, "view"), Vec2::new(800.0, 600.0));
}

#[test]
fn viewport_events_must_be_finite_and_positive() {
    let s = InputSampler::new(map());
    for (width, height) in [
        (0.0, 10.0),
        (10.0, -1.0),
        (f32::NAN, 10.0),
        (10.0, f32::INFINITY),
    ] {
        let err = s
            .check(0, &InputEvent::Viewport { width, height })
            .unwrap_err();
        assert!(err.to_string().contains("viewport"), "{err}");
    }
}

#[test]
fn viewport_binding_fits_only_axis2d() {
    let bad = InputMap::single("view", ActionKind::Button, vec![Binding::Viewport]);
    assert!(bad.is_err());
}

#[test]
fn consumed_pointer_hides_only_pointer_actions() {
    let map = map();
    let mut s = InputSampler::new(map.clone());
    let mut consumed = ConsumedActions::new(&map);
    s.push(
        0,
        InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        },
    )
    .unwrap();
    s.push(
        0,
        InputEvent::Key {
            key: KeyCode::Space,
            pressed: true,
        },
    )
    .unwrap();
    let states = s.sample(0).clone();
    assert!(states.unconsumed("move", &consumed).unwrap().just_pressed);
    consumed.consume_pointer();
    assert!(consumed.pointer_consumed());
    // Pointer-bound actions are hidden; "fire" has a mouse binding too, so it counts.
    for name in ["move", "aim", "zoom", "fire", "view"] {
        let hidden = states.unconsumed(name, &consumed).is_none();
        assert_eq!(hidden, name != "view", "{name}");
    }
    assert!(states.unconsumed("jump", &consumed).unwrap().just_pressed);
    assert!(states.unconsumed("nope", &consumed).is_none());
    consumed.clear();
    assert!(states.unconsumed("move", &consumed).is_some());
}

#[test]
fn starter_map_has_the_ui_actions() {
    let map = InputMap::starter();
    map.validate().unwrap();
    let kind = |n: &str| map.action(n).unwrap_or_else(|| panic!("{n}")).kind;
    for (name, want) in [
        ("move", ActionKind::Button),
        ("pointer_ray", ActionKind::Ray),
        ("ui_pointer", ActionKind::Axis2d),
        ("ui_click", ActionKind::Button),
        ("ui_scroll", ActionKind::Axis2d),
        ("ui_viewport", ActionKind::Axis2d),
        ("ui_up", ActionKind::Button),
        ("ui_down", ActionKind::Button),
        ("ui_left", ActionKind::Button),
        ("ui_right", ActionKind::Button),
        ("ui_accept", ActionKind::Button),
        ("ui_cancel", ActionKind::Button),
        ("ui_next", ActionKind::Button),
        ("ui_prev", ActionKind::Button),
        ("ui_page_up", ActionKind::Button),
        ("ui_page_down", ActionKind::Button),
    ] {
        assert_eq!(kind(name), want, "{name}");
    }
    assert_eq!(map.actions.len(), 16);
    let bindings = |n: &str| map.action(n).unwrap().bindings.clone();
    assert_eq!(bindings("ui_viewport"), vec![Binding::Viewport]);
    assert_eq!(
        bindings("ui_accept"),
        vec![Binding::Key(KeyCode::Enter), Binding::Key(KeyCode::Space)]
    );
    assert_eq!(bindings("ui_page_up"), vec![Binding::Key(KeyCode::PageUp)]);
    assert_eq!(
        bindings("ui_prev"),
        vec![Binding::Chord {
            modifiers: vec![concordia_input::Modifier::Shift],
            key: KeyCode::Tab
        }]
    );
}

#[test]
fn navigation_keys_serialize_by_name() {
    for (key, name) in [
        (KeyCode::PageUp, "PageUp"),
        (KeyCode::PageDown, "PageDown"),
        (KeyCode::Home, "Home"),
        (KeyCode::End, "End"),
    ] {
        assert_eq!(serde_json::to_value(key).unwrap(), name);
    }
}
