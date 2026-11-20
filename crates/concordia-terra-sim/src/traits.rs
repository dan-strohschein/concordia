//! Traits: Rust types with reflected parameters that add components to a template.

use std::collections::BTreeMap;

use concordia_reflect::{DynReflect, Reflect, RegistryError, TypeRegistry};
use concordia_terra::{Component, ReflectComponent};
use serde_json::Value;

use crate::error::TemplateError;

/// A Mass-style trait: configured from text, it adds components (and needs processors).
pub trait EntityTrait: Reflect {
    /// Adds this trait's components.
    fn build(&self, template: &mut TemplateBuilder<'_>) -> Result<(), TemplateError>;

    /// Processors or systems (by name) the trait's entities need to behave.
    fn requires(&self) -> Vec<String> {
        Vec::new()
    }
}

type BuildFn = fn(&dyn DynReflect, &mut TemplateBuilder<'_>) -> Result<Vec<String>, TemplateError>;

/// Registry type data that marks a type as a trait and builds it type-erased.
#[derive(Clone, Copy)]
pub struct ReflectTrait {
    build: BuildFn,
}

impl ReflectTrait {
    fn of<T: EntityTrait>() -> Self {
        Self {
            build: |value, builder| {
                let Some(t) = value.as_any().downcast_ref::<T>() else {
                    return Err(TemplateError::InvalidValue {
                        what: T::type_info().name.to_owned(),
                        reason: format!("expected a {} value", T::type_info().name),
                    });
                };
                t.build(builder)?;
                Ok(t.requires())
            },
        }
    }

    /// Adds the trait's components to `builder`; returns the processors it requires.
    pub(crate) fn build(
        &self,
        value: &dyn DynReflect,
        builder: &mut TemplateBuilder<'_>,
    ) -> Result<Vec<String>, TemplateError> {
        (self.build)(value, builder)
    }
}

impl std::fmt::Debug for ReflectTrait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReflectTrait")
    }
}

/// Registers `T` in `registry` as a trait.
pub fn register_trait<T: EntityTrait>(registry: &mut TypeRegistry) -> Result<(), RegistryError> {
    registry
        .register::<T>()?
        .insert_data(ReflectTrait::of::<T>());
    Ok(())
}

/// Every registered trait name, sorted.
pub fn trait_names(registry: &TypeRegistry) -> impl Iterator<Item = &'static str> + '_ {
    let mut names: Vec<&'static str> = registry.names().filter(|n| is_trait(registry, n)).collect();
    names.sort_unstable();
    names.into_iter()
}

/// The JSON Schema of trait `name`'s parameters.
pub fn trait_schema(registry: &TypeRegistry, name: &str) -> Option<Value> {
    is_trait(registry, name)
        .then(|| registry.schema_for(name))
        .flatten()
}

fn is_trait(registry: &TypeRegistry, name: &str) -> bool {
    registry
        .get_by_name(name)
        .is_some_and(|r| r.data::<ReflectTrait>().is_some())
}

/// One component a template adds.
pub(crate) struct Part {
    pub(crate) value: Box<dyn DynReflect>,
    pub(crate) component: ReflectComponent,
    pub(crate) shared: bool,
    /// The trait that added it, or `components` for an explicit value.
    pub(crate) from: String,
}

/// Collects the components traits add, catching conflicts between traits.
pub struct TemplateBuilder<'r> {
    registry: &'r TypeRegistry,
    current: String,
    pub(crate) parts: BTreeMap<String, Part>,
}

impl<'r> TemplateBuilder<'r> {
    pub(crate) fn new(registry: &'r TypeRegistry) -> Self {
        Self {
            registry,
            current: String::new(),
            parts: BTreeMap::new(),
        }
    }

    /// Following adds come from `source` (a trait name, or `components`).
    pub(crate) fn source(&mut self, source: &str) {
        self.current = source.to_owned();
    }

    /// Adds a component.
    pub fn add<C: Component>(&mut self, value: C) -> Result<(), TemplateError> {
        self.put(Box::new(value), false)
    }

    /// Adds a shared component (one stored value per archetype, where the world allows it).
    pub fn add_shared<C: Component>(&mut self, value: C) -> Result<(), TemplateError> {
        self.put(Box::new(value), true)
    }

    /// Adds `value`; a second trait adding the same component is a conflict, while explicit
    /// component values replace what traits added.
    pub(crate) fn put(
        &mut self,
        value: Box<dyn DynReflect>,
        shared: bool,
    ) -> Result<(), TemplateError> {
        let name = value.info().name;
        let component = self
            .registry
            .get_by_name(name)
            .and_then(|r| r.data::<ReflectComponent>().copied())
            .ok_or_else(|| TemplateError::UnknownComponent {
                name: name.to_owned(),
                did_you_mean: None,
            })?;
        let explicit = self.current == crate::template::COMPONENTS;
        if let Some(existing) = self.parts.get(name)
            && !explicit
        {
            let (first, second) = ordered(&existing.from, &self.current);
            return Err(TemplateError::TraitConflict {
                component: name.to_owned(),
                first,
                second,
            });
        }
        let shared = shared || (explicit && self.parts.get(name).is_some_and(|p| p.shared));
        self.parts.insert(
            name.to_owned(),
            Part {
                value,
                component,
                shared,
                from: self.current.clone(),
            },
        );
        Ok(())
    }
}

fn ordered(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}
