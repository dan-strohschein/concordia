//! Pointer rays as input (M2 phase 2.5b.2a, ADR-0078): the `Ray` kind, the `PointerRay` event and
//! binding, and checks on bad rays.
//!
//! A ray's direction is stored normalized: (0, −2, 0) becomes (0, −1, 0), and (3, 0, 4) becomes
//! (0.6, 0, 0.8). A ray action is inactive (zero) until the first ray arrives, then pressed from
//! then on, with `just_pressed` on that first tick only.

use concordia_core::math::Vec3;
use concordia_input::{ActionKind, ActionValue, InputError, InputEvent, InputMap, InputSampler};

const MAP: &str = r#"InputMap(
    actions: {
        "aim": (kind: Ray, bindings: [PointerRay]),
        "go": (kind: Button, bindings: [MouseButton(Left)]),
    },
)"#;

fn sampler() -> InputSampler {
    InputSampler::new(InputMap::parse(MAP).unwrap())
}

fn ray(origin: Vec3, direction: Vec3) -> InputEvent {
    InputEvent::PointerRay { origin, direction }
}

fn value(origin: Vec3, direction: Vec3) -> ActionValue {
    ActionValue::Ray { origin, direction }
}

#[test]
fn pointer_ray_event_sets_a_ray_action() {
    let mut s = sampler();
    let idle = *s.sample(0).get("aim").unwrap();
    assert_eq!(idle.value, ActionValue::zero(ActionKind::Ray));
    assert!(!idle.pressed && !idle.just_pressed);

    s.push(1, ray(Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, -2.0, 0.0)))
        .unwrap();
    let first = *s.sample(1).get("aim").unwrap();
    assert_eq!(first.value, value(Vec3::new(0.0, 10.0, 0.0), Vec3::NEG_Y));
    assert!(first.pressed && first.just_pressed);
    let held = *s.sample(2).get("aim").unwrap();
    assert_eq!(held.value, first.value, "the ray holds");
    assert!(held.pressed && !held.just_pressed);

    s.push(3, ray(Vec3::ONE, Vec3::new(3.0, 0.0, 4.0))).unwrap();
    let moved = *s.sample(3).get("aim").unwrap();
    assert_eq!(moved.value, value(Vec3::ONE, Vec3::new(0.6, 0.0, 0.8)));
    assert!(moved.pressed && !moved.just_pressed);

    // A direct action value is normalized the same way and holds until set again.
    let forced = InputEvent::Action {
        action: "aim".into(),
        value: value(Vec3::ZERO, Vec3::new(0.0, 0.0, -5.0)),
    };
    s.push(4, forced).unwrap();
    assert_eq!(
        s.sample(4).get("aim").unwrap().value,
        value(Vec3::ZERO, Vec3::NEG_Z)
    );
    assert_eq!(
        s.sample(4).get("go").unwrap().value,
        ActionValue::Button(false)
    );
}

#[test]
fn ray_bindings_fit_only_ray_actions() {
    for text in [
        r#"InputMap(actions: {"aim": (kind: Axis2d, bindings: [PointerRay])})"#,
        r#"InputMap(actions: {"aim": (kind: Ray, bindings: [PointerPosition])})"#,
        r#"InputMap(actions: {"aim": (kind: Ray, bindings: [MouseButton(Left)])})"#,
    ] {
        match InputMap::parse(text) {
            Err(InputError::BindingKind { action, .. }) => assert_eq!(action, "aim"),
            other => panic!("{text}: {other:?}"),
        }
    }
}

#[test]
fn inject_rejects_bad_rays() {
    let s = sampler();
    let bad = [
        ray(Vec3::new(f32::NAN, 0.0, 0.0), Vec3::X),
        ray(Vec3::ZERO, Vec3::ZERO),
        ray(Vec3::ZERO, Vec3::new(f32::INFINITY, 0.0, 0.0)),
        InputEvent::Action {
            action: "aim".into(),
            value: value(Vec3::ZERO, Vec3::ZERO),
        },
    ];
    for event in bad {
        match s.check(0, &event) {
            Err(InputError::Pointer(message)) => {
                assert!(message.contains("ray"), "{message}")
            }
            other => panic!("{event:?}: {other:?}"),
        }
    }
    assert!(s.check(0, &ray(Vec3::ZERO, Vec3::X)).is_ok());
}
