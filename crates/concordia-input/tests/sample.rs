//! Deterministic per-tick sampling (M1 phase 1.1).

use std::time::Duration;

use concordia_core::math::Vec2;
use concordia_core::time::TickRate;
use concordia_input::{
    ActionValue, InputError, InputEvent, InputMap, InputSampler, KeyCode, MouseButton,
    tick_for_time,
};

const DEFAULT: &str = include_str!("../../../examples/m1-physics-stack/input/default.input.ron");

fn sampler() -> InputSampler {
    InputSampler::new(InputMap::parse(DEFAULT).unwrap())
}

fn key(key: KeyCode, pressed: bool) -> InputEvent {
    InputEvent::Key { key, pressed }
}

fn click(pressed: bool) -> InputEvent {
    InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed,
    }
}

#[test]
fn button_action_reports_just_pressed_for_exactly_one_tick() {
    let mut s = sampler();
    s.push(0, key(KeyCode::Space, true)).unwrap();
    let t0 = s.sample(0).get("spawn_box").copied().unwrap();
    assert!(t0.pressed && t0.just_pressed && !t0.just_released);
    let t1 = s.sample(1).get("spawn_box").copied().unwrap();
    assert!(t1.pressed && !t1.just_pressed);
    s.push(2, key(KeyCode::Space, false)).unwrap();
    let t2 = s.sample(2).get("spawn_box").copied().unwrap();
    assert!(!t2.pressed && t2.just_released);
}

#[test]
fn axis2d_from_wasd_is_normalized_on_diagonals() {
    let mut s = sampler();
    s.push(0, key(KeyCode::W, true)).unwrap();
    s.push(0, key(KeyCode::D, true)).unwrap();
    let ActionValue::Axis2d(v) = s.sample(0).get("camera_pan").unwrap().value else {
        panic!("camera_pan is Axis2d");
    };
    assert!((v.length() - 1.0).abs() < 1e-6, "{v}");
    assert!((v.x - v.y).abs() < 1e-6 && v.x > 0.0, "{v}");
}

#[test]
fn events_between_ticks_are_applied_at_next_tick_boundary() {
    let rate = TickRate::HZ_60;
    let dt = Duration::from_secs(1) / 60;
    assert_eq!(tick_for_time(dt * 5, rate), 5);
    assert_eq!(tick_for_time(dt * 5 + Duration::from_micros(1), rate), 6);
    let mut s = sampler();
    let at = tick_for_time(dt * 5 + Duration::from_millis(3), rate);
    s.push(at, key(KeyCode::Space, true)).unwrap();
    for tick in 0..6 {
        assert!(
            !s.sample(tick).get("spawn_box").unwrap().pressed,
            "tick {tick}"
        );
    }
    assert!(s.sample(6).get("spawn_box").unwrap().just_pressed);
}

#[test]
fn press_and_release_within_one_tick_still_registers_just_pressed() {
    let mut s = sampler();
    s.push(3, click(true)).unwrap();
    s.push(3, click(false)).unwrap();
    let state = s.sample(3).get("select").copied().unwrap();
    assert!(state.just_pressed && state.just_released && !state.pressed);
}

#[test]
fn injected_click_produces_same_action_state_as_os_click() {
    let rate = TickRate::HZ_60;
    let mut os = sampler();
    let tick = tick_for_time(Duration::from_millis(40), rate);
    os.push(tick, click(true)).unwrap();
    let mut injected = sampler();
    injected.push(tick, click(true)).unwrap();
    for t in 0..=tick {
        assert_eq!(os.sample(t), injected.sample(t));
    }
}

#[test]
fn inject_at_future_tick_is_applied_at_that_tick() {
    let mut s = sampler();
    s.push(10, key(KeyCode::Space, true)).unwrap();
    assert!(!s.sample(9).get("spawn_box").unwrap().pressed);
    assert!(s.sample(10).get("spawn_box").unwrap().just_pressed);
}

#[test]
fn chord_needs_its_modifier() {
    let mut s = sampler();
    s.push(0, key(KeyCode::B, true)).unwrap();
    assert!(!s.sample(0).get("spawn_box").unwrap().pressed);
    s.push(1, key(KeyCode::ShiftLeft, true)).unwrap();
    assert!(s.sample(1).get("spawn_box").unwrap().just_pressed);
}

#[test]
fn action_events_bypass_bindings_and_unknown_actions_are_rejected() {
    let mut s = sampler();
    let set = |v| InputEvent::Action {
        action: "camera_pan".into(),
        value: ActionValue::Axis2d(v),
    };
    s.push(0, set(Vec2::new(0.0, -1.0))).unwrap();
    assert_eq!(
        s.sample(0).get("camera_pan").unwrap().value,
        ActionValue::Axis2d(Vec2::new(0.0, -1.0))
    );
    let unknown = InputEvent::Action {
        action: "fly".into(),
        value: ActionValue::Button(true),
    };
    assert!(matches!(
        s.push(1, unknown),
        Err(InputError::UnknownAction { known, .. }) if known.contains(&"camera_pan".to_owned())
    ));
    let wrong = InputEvent::Action {
        action: "select".into(),
        value: ActionValue::Axis1d(1.0),
    };
    assert!(matches!(
        s.push(1, wrong),
        Err(InputError::ValueKind { .. })
    ));
}

#[test]
fn past_ticks_are_rejected() {
    let mut s = sampler();
    s.sample(0);
    s.sample(1);
    assert_eq!(s.next_tick(), 2);
    assert!(matches!(
        s.push(1, click(true)),
        Err(InputError::TickInPast { tick: 1, next: 2 })
    ));
}

#[test]
fn pointer_and_scroll_feed_axes_per_tick() {
    let mut s = sampler();
    s.push(0, InputEvent::PointerMove { x: 120.0, y: 40.0 })
        .unwrap();
    s.push(0, InputEvent::Scroll { dx: 0.0, dy: 2.0 }).unwrap();
    let states = s.sample(0).clone();
    assert_eq!(
        states.get("pointer").unwrap().value,
        ActionValue::Axis2d(Vec2::new(120.0, 40.0))
    );
    assert_eq!(
        states.get("camera_zoom").unwrap().value,
        ActionValue::Axis1d(2.0)
    );
    let next = s.sample(1).clone();
    assert_eq!(
        next.get("camera_zoom").unwrap().value,
        ActionValue::Axis1d(0.0)
    );
    assert_eq!(
        next.get("pointer").unwrap().value,
        ActionValue::Axis2d(Vec2::new(120.0, 40.0))
    );
}

#[test]
fn frame_lists_every_action_sorted_by_id() {
    let mut s = sampler();
    s.push(0, click(true)).unwrap();
    let frame = s.sample(0).frame();
    let ids: Vec<u32> = frame.0.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, [0, 1, 2, 3, 4]);
    assert_eq!(frame.0[3].1, ActionValue::Button(true));
}
