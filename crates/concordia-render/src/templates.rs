//! The render module's ready-made entities: primitives, a camera and lights (M1 phase 1.13).

use concordia_api::Template;
use concordia_assets::MeshRenderer;
use concordia_core::math::{Quat, Vec3};
use concordia_scene::builtin::Transform;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{Camera, DirectionalLight, PointLight, SpotLight};

/// Primitives, a camera and the three light kinds.
pub(crate) fn templates() -> Vec<Template> {
    let mut out: Vec<Template> = [
        ("Cube", "cube", "A 1 m cube mesh with the default material."),
        (
            "Sphere",
            "sphere",
            "A 1 m diameter sphere mesh with the default material.",
        ),
        (
            "Plane",
            "plane",
            "A 1 m square floor plane (XZ, facing up); scale it for a ground.",
        ),
    ]
    .into_iter()
    .map(|(name, mesh, about)| primitive(name, mesh, about))
    .collect();
    out.push(with(
        "Camera",
        "A perspective camera looking down -Z; the highest-priority camera renders the game.",
        Transform {
            translation: Vec3::new(0.0, 2.0, 6.0),
            ..Transform::default()
        },
        [("Camera", value(Camera::default()))],
    ));
    out.push(with(
        "Directional Light",
        "A sun: parallel light from one direction (aimed 45° down by default), with lux intensity.",
        Transform {
            rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_4),
            ..Transform::default()
        },
        [("DirectionalLight", value(DirectionalLight::default()))],
    ));
    out.push(with(
        "Point Light",
        "A bulb: light in every direction from a point, fading out by its range.",
        Transform {
            translation: Vec3::new(0.0, 2.0, 0.0),
            ..Transform::default()
        },
        [("PointLight", value(PointLight::default()))],
    ));
    out.push(with(
        "Spot Light",
        "A cone of light along -Z from a point (inner and outer angles in radians).",
        Transform {
            translation: Vec3::new(0.0, 3.0, 0.0),
            rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            ..Transform::default()
        },
        [("SpotLight", value(SpotLight::default()))],
    ));
    out
}

fn primitive(name: &str, mesh: &str, about: &str) -> Template {
    let renderer = json!({
        "mesh": format!("builtin:mesh/{mesh}"),
        "material": "builtin:material/default",
        "cast_shadows": true,
    });
    debug_assert!(serde_json::from_value::<MeshRenderer>(renderer.clone()).is_ok());
    with(
        name,
        about,
        Transform::default(),
        [("MeshRenderer", renderer)],
    )
}

/// A template with a name, a transform and `extra` components.
fn with<const N: usize>(
    name: &str,
    about: &str,
    transform: Transform,
    extra: [(&'static str, Value); N],
) -> Template {
    let components = [("Name", json!(name)), ("Transform", value(transform))]
        .into_iter()
        .chain(extra);
    Template::new(name, about, components)
}

fn value(v: impl Serialize) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}
