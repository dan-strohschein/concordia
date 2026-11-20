//! Test traits and helpers.
#![allow(dead_code, missing_docs)]

use concordia_core::math::Vec3;
use concordia_reflect::{Reflect, TypeRegistry};
use concordia_scene::builtin::{Transform, Velocity};
use concordia_terra::register_component;
use concordia_terra_sim::{EntityTrait, TemplateBuilder, TemplateError, register_trait};
use serde::{Deserialize, Serialize};

/// A shared team id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Team(pub u32);

/// Tag: hostile to the player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Hostile;

/// Trait: moves along +x at `speed`; needs the velocity integrator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Moving {
    /// Meters per second.
    #[reflect(range(min = 0.0, max = 100.0))]
    pub speed: f32,
}

impl EntityTrait for Moving {
    fn build(&self, template: &mut TemplateBuilder<'_>) -> Result<(), TemplateError> {
        template.add(Transform::default())?;
        template.add(Velocity {
            linear: Vec3::new(self.speed, 0.0, 0.0),
            angular: Vec3::ZERO,
        })
    }

    fn requires(&self) -> Vec<String> {
        vec!["integrate_velocity".to_owned()]
    }
}

/// Trait: belongs to a team (shared) and is hostile (tag).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Faction {
    /// Team id.
    pub team: u32,
}

impl EntityTrait for Faction {
    fn build(&self, template: &mut TemplateBuilder<'_>) -> Result<(), TemplateError> {
        template.add_shared(Team(self.team))?;
        template.add(Hostile)
    }
}

/// Trait that also sets a velocity: conflicts with `Moving`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Drifting {}

impl EntityTrait for Drifting {
    fn build(&self, template: &mut TemplateBuilder<'_>) -> Result<(), TemplateError> {
        template.add(Velocity::default())
    }
}

/// Built-ins plus the test components and traits.
pub fn registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    concordia_terra_sim::register(&mut registry).unwrap();
    register_component::<Team>(&mut registry).unwrap();
    register_component::<Hostile>(&mut registry).unwrap();
    register_trait::<Moving>(&mut registry).unwrap();
    register_trait::<Faction>(&mut registry).unwrap();
    register_trait::<Drifting>(&mut registry).unwrap();
    registry
}

/// A goblin template as text: two traits plus a name.
pub const GOBLIN: &str = r#"Template(
    format: 1,
    description: "A small hostile walker.",
    traits: {
        "Faction": (
            team: 2,
        ),
        "Moving": (
            speed: 1.5,
        ),
    },
    components: {
        "Name": "Goblin",
    },
)
"#;
