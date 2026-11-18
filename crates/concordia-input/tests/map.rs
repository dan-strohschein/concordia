//! Input maps (`.input.ron`) (M1 phase 1.1).

use concordia_input::{ActionKind, Binding, InputError, InputMap, KeyCode};

const DEFAULT: &str = include_str!("../../../examples/m1-physics-stack/input/default.input.ron");

#[test]
fn input_map_parses_default_bindings() {
    let map = InputMap::parse(DEFAULT).unwrap();
    assert_eq!(
        map.action_names().collect::<Vec<_>>(),
        [
            "camera_pan",
            "camera_zoom",
            "pointer",
            "select",
            "spawn_box"
        ]
    );
    assert_eq!(map.action("select").unwrap().kind, ActionKind::Button);
    insta::assert_snapshot!(map.to_ron().unwrap());
}

#[test]
fn canonical_form_round_trips() {
    let map = InputMap::parse(DEFAULT).unwrap();
    let text = map.to_ron().unwrap();
    assert_eq!(InputMap::parse(&text).unwrap(), map);
}

#[test]
fn binding_must_fit_action_kind() {
    let text = r#"InputMap(actions: {"jump": (kind: Button, bindings: [KeyAxis2d(up: W, down: S, left: A, right: D)])})"#;
    let err = InputMap::parse(text).unwrap_err();
    assert!(
        matches!(&err, InputError::BindingKind { action, .. } if action == "jump"),
        "{err}"
    );
    assert!(err.to_string().contains("Button"), "{err}");
}

#[test]
fn action_names_are_validated() {
    for bad in ["", "Jump", "9lives", "has space", &"a".repeat(65)] {
        let text =
            format!(r#"InputMap(actions: {{"{bad}": (kind: Button, bindings: [Key(Space)])}})"#);
        assert!(
            matches!(InputMap::parse(&text), Err(InputError::ActionName { .. })),
            "{bad:?}"
        );
    }
}

#[test]
fn syntax_errors_carry_line_numbers() {
    let err = InputMap::parse(
        "InputMap(\n  actions: {\n    \"a\": (kind: Buton, bindings: []),\n  },\n)",
    )
    .unwrap_err();
    assert!(err.to_string().contains("line 3"), "{err}");
}

#[test]
fn key_binding_serializes_by_name() {
    let map = InputMap::single(
        "jump",
        ActionKind::Button,
        vec![Binding::Key(KeyCode::Space)],
    )
    .unwrap();
    assert!(map.to_ron().unwrap().contains("Key(Space)"));
}

#[test]
fn example_input_map_is_canonical() {
    let map = InputMap::parse(DEFAULT).unwrap();
    assert_eq!(map.to_ron().unwrap(), DEFAULT);
}
