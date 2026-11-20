//! Terra templates, traits and spawners (M2 phase 2.0c): Mass-style traits are Rust types with
//! reflected parameters that add components to a template; `.template.ron` files compose
//! traits and component values; spawners place a template's entities with a seeded
//! distribution when play starts. Concepts from Epic's public Mass documentation only.

mod catalog;
mod error;
mod movement;
mod spawner;
mod template;
mod traits;

pub use catalog::{TEMPLATE_DIR, TemplateCatalog};
pub use error::TemplateError;
pub use movement::Movement;
pub use spawner::{Distribution, Spawner, expand_spawners};
pub use template::{CompiledTemplate, Template};
pub use traits::{
    DynamicTrait, EntityTrait, ReflectTrait, TemplateBuilder, register_dynamic_trait,
    register_trait, trait_names, trait_schema,
};

use concordia_reflect::{RegistryError, TypeRegistry};

/// Registers this crate's component (`Spawner`) and built-in trait (`Movement`).
pub fn register(registry: &mut TypeRegistry) -> Result<(), RegistryError> {
    concordia_terra::register_component::<Spawner>(registry)?;
    register_trait::<Movement>(registry)
}
