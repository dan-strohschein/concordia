//! Agent-actionable read errors with positions (M0 phase 0.5).

mod common;

use common::{fixture, registry};
use concordia_scene::{SceneError, read_prefab, read_scene};

/// 1-based (line, col) of the first occurrence of `needle`.
fn position_of(text: &str, needle: &str) -> (usize, usize) {
    let offset = text.find(needle).expect("needle present");
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().unwrap().chars().count() + 1;
    (line, col)
}

#[test]
fn unknown_component_reports_location_and_suggestion() {
    let text = fixture("builtins.scene.ron").replacen("\"Transform\"", "\"Transfrom\"", 1);
    let (line, col) = position_of(&text, "\"Transfrom\"");
    let err = read_scene(&text, &registry()).unwrap_err();
    assert_eq!(
        err,
        SceneError::UnknownComponent {
            name: "Transfrom".into(),
            line,
            col,
            did_you_mean: Some("Transform".into())
        }
    );
    assert!(
        err.to_string().contains("did you mean \"Transform\""),
        "{err}"
    );
}

#[test]
fn invalid_value_reports_line_col_and_expected_schema() {
    let text = fixture("builtins.scene.ron").replacen(
        "translation: (5.0, 0.0, 0.0)",
        "translation: \"far\"",
        1,
    );
    let (line, col) = position_of(&text, "\"far\"");
    let err = read_scene(&text, &registry()).unwrap_err();
    assert_eq!(
        err,
        SceneError::InvalidValue {
            component: "Transform".into(),
            pointer: "/translation".into(),
            expected: "a tuple of 3".into(),
            found: "a string".into(),
            line,
            col,
        }
    );
}

#[test]
fn unknown_field_names_the_valid_fields() {
    let text = fixture("builtins.scene.ron").replacen(
        "scale: (1.0, 1.0, 1.0)",
        "scal: (1.0, 1.0, 1.0)",
        1,
    );
    let (line, _) = position_of(&text, "scal:");
    let err = read_scene(&text, &registry()).unwrap_err();
    let SceneError::InvalidValue {
        pointer,
        expected,
        line: at,
        ..
    } = &err
    else {
        panic!("{err:?}")
    };
    assert_eq!((pointer.as_str(), *at), ("", line));
    assert!(
        expected.contains("rotation, scale, translation"),
        "{expected}"
    );
}

#[test]
fn duplicate_entity_id_rejected() {
    let text = fixture("builtins.scene.ron").replace("-000000000003\"", "-000000000002\"");
    let err = read_scene(&text, &registry()).unwrap_err();
    assert!(
        matches!(err, SceneError::DuplicateEntity { ref id, .. } if id.to_string().ends_with("000000000002")),
        "{err:?}"
    );
}

#[test]
fn newer_format_version_rejected_with_upgrade_hint() {
    let text = fixture("builtins.scene.ron").replacen("format: 1", "format: 2", 1);
    let err = read_scene(&text, &registry()).unwrap_err();
    assert_eq!(
        err,
        SceneError::UnsupportedFormat {
            found: 2,
            supported: 1
        }
    );
    assert!(err.to_string().contains("newer"), "{err}");
}

#[test]
fn syntax_errors_report_position() {
    let err = read_scene("Scene(\n    format: 1,\n    id: @\n)", &registry()).unwrap_err();
    assert!(
        matches!(
            err,
            SceneError::Parse {
                line: 3,
                col: 9,
                ..
            }
        ),
        "{err:?}"
    );
    let err = read_scene("Scene(format: 1", &registry()).unwrap_err();
    assert!(matches!(err, SceneError::Parse { .. }), "{err:?}");
}

#[test]
fn wrong_document_kind_is_rejected() {
    let prefab = fixture("crates.prefab.ron");
    assert!(matches!(
        read_scene(&prefab, &registry()),
        Err(SceneError::Parse { .. })
    ));
    assert!(matches!(
        read_prefab(&fixture("builtins.scene.ron"), &registry()),
        Err(SceneError::Parse { .. })
    ));
}

#[test]
fn bad_entity_id_is_reported() {
    let text = fixture("builtins.scene.ron").replacen(
        "7e40-7000-8000-000000000002",
        "7e40-4000-8000-000000000002",
        1,
    );
    assert!(matches!(
        read_scene(&text, &registry()),
        Err(SceneError::Parse { .. })
    ));
}
