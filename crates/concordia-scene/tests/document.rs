//! Typed RON documents beyond scenes: input maps, materials, sidecars (M1 phase 1.1).

use std::collections::BTreeMap;

use concordia_reflect::Reflect;
use concordia_scene::{SceneError, read_document, write_document};
use serde::{Deserialize, Serialize};

/// Movement mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
enum Mode {
    /// Fast.
    Fast,
    /// Slow by a factor.
    Slow {
        /// Slowdown.
        factor: f32,
    },
}

/// A test document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
struct Doc {
    /// Name.
    name: String,
    /// Speed.
    speed: f32,
    /// Modes by key.
    modes: BTreeMap<String, Mode>,
    /// Tags.
    tags: Vec<String>,
}

fn doc() -> Doc {
    Doc {
        name: "walk".into(),
        speed: 1.5,
        modes: BTreeMap::from([
            ("a".to_owned(), Mode::Fast),
            ("b".to_owned(), Mode::Slow { factor: 0.5 }),
        ]),
        tags: vec!["x".into(), "y".into()],
    }
}

#[test]
fn document_round_trips_and_is_canonical() {
    let text = write_document(&doc()).unwrap();
    assert!(text.starts_with("Doc(\n"), "{text}");
    assert_eq!(read_document::<Doc>(&text).unwrap(), doc());
    let again = write_document(&read_document::<Doc>(&text).unwrap()).unwrap();
    assert_eq!(again, text);
}

#[test]
fn unnamed_root_is_accepted() {
    let text = r#"(name: "walk", speed: 1.5, modes: {"a": Fast, "b": Slow(factor: 0.5)}, tags: ["x", "y"])"#;
    assert_eq!(read_document::<Doc>(text).unwrap(), doc());
}

#[test]
fn unknown_field_reports_position_and_valid_fields() {
    let text = "Doc(\n    name: \"walk\",\n    sped: 1.0,\n)";
    let err = read_document::<Doc>(text).unwrap_err();
    let shown = err.to_string();
    assert!(shown.contains("sped"), "{shown}");
    match err {
        SceneError::InvalidValue { line, .. } | SceneError::Parse { line, .. } => {
            assert_eq!(line, 3, "{shown}")
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn wrong_type_names_the_pointer() {
    let text = r#"Doc(name: "walk", speed: "fast", modes: {}, tags: [])"#;
    let SceneError::InvalidValue { pointer, .. } = read_document::<Doc>(text).unwrap_err() else {
        panic!("expected InvalidValue");
    };
    assert_eq!(pointer, "/speed");
}

#[test]
fn non_finite_values_are_not_written() {
    let mut bad = doc();
    bad.speed = f32::NAN;
    assert!(matches!(
        write_document(&bad),
        Err(SceneError::NonFiniteValue { .. })
    ));
}
