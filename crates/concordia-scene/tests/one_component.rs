//! Reading one component type from a scene without a registry (M4 4.1b4: the main camera's
//! exposure, read when a project loads, before scripts have registered their components).

use concordia_core::math::Vec3;
use concordia_scene::builtin::Transform;
use concordia_scene::read_components;

const SCENE: &str = r#"Scene(
    format: 1,
    id: "0193a1c2-7e40-7000-8000-000000000001",
    name: "Mixed",
    entities: [
        Entity(
            id: "0193a1c2-7e40-7000-8000-000000000002",
            components: {
                "game.Mystery": (anything: [1, 2, 3]),
                "Transform": (rotation: (0.0, 0.0, 0.0, 1.0), scale: (1.0, 1.0, 1.0), translation: (1.0, 2.0, 3.0)),
            },
        ),
        Entity(
            id: "0193a1c2-7e40-7000-8000-000000000003",
            components: {"game.Other": ()},
        ),
    ],
)"#;

#[test]
fn reads_one_component_type_and_ignores_the_rest() {
    let found: Vec<(_, Transform)> = read_components(SCENE, "Transform").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].0.to_string(),
        "0193a1c2-7e40-7000-8000-000000000002"
    );
    assert_eq!(found[0].1.translation, Vec3::new(1.0, 2.0, 3.0));
    let none: Vec<(_, Transform)> = read_components(SCENE, "Camera").unwrap();
    assert!(none.is_empty());
}

#[test]
fn a_malformed_component_is_an_error_naming_it() {
    let bad = SCENE.replace("translation: (1.0, 2.0, 3.0)", "translation: \"up\"");
    let err = read_components::<Transform>(&bad, "Transform").unwrap_err();
    assert!(err.to_string().contains("Transform"), "{err}");
    assert!(read_components::<Transform>("not a scene", "Transform").is_err());
}
