//! Traits: Rust types with reflected parameters that add components to a template.

use std::collections::BTreeMap;
use std::sync::Arc;

use concordia_reflect::{
    DynReflect, Reflect, RegistryError, Shape, ShapeDesc, TypeDesc, TypeRegistry,
};
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

type BuildFn = dyn Fn(&dyn DynReflect, &mut TemplateBuilder<'_>) -> Result<Vec<String>, TemplateError>
    + Send
    + Sync;

/// Registry type data that marks a type as a trait and builds it type-erased.
#[derive(Clone)]
pub struct ReflectTrait {
    build: Arc<BuildFn>,
}

impl ReflectTrait {
    fn of<T: EntityTrait>() -> Self {
        Self {
            build: Arc::new(|value, builder| {
                let Some(t) = value.as_any().downcast_ref::<T>() else {
                    return Err(TemplateError::InvalidValue {
                        what: T::type_info().name.to_owned(),
                        reason: format!("expected a {} value", T::type_info().name),
                    });
                };
                t.build(builder)?;
                Ok(t.requires())
            }),
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

/// A trait described at runtime (by a script module): components added with their type
/// defaults, and the processors its entities need. It takes no parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DynamicTrait {
    /// Stable name.
    pub name: String,
    /// Doc text.
    pub docs: String,
    /// Component names; each needs a type default.
    pub components: Vec<String>,
    /// Processors its entities need.
    pub requires: Vec<String>,
}

/// Registers (or replaces) `spec` as a parameterless trait in `registry`.
pub fn register_dynamic_trait(
    registry: &mut TypeRegistry,
    spec: &DynamicTrait,
) -> Result<(), TemplateError> {
    let values = spec
        .components
        .iter()
        .map(|name| default_component(registry, name))
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(existing) = registry.get_by_name(&spec.name)
        && existing.data::<ReflectTrait>().is_none()
    {
        return Err(TemplateError::InvalidValue {
            what: spec.name.clone(),
            reason: "is already a registered type that is not a trait".into(),
        });
    }
    let requires = spec.requires.clone();
    let build = ReflectTrait {
        build: Arc::new(move |_, builder| {
            for value in &values {
                builder.put(value.clone_dyn(), false)?;
            }
            Ok(requires.clone())
        }),
    };
    registry
        .register_dynamic(&parameterless(spec))
        .map_err(|e| TemplateError::InvalidValue {
            what: spec.name.clone(),
            reason: e.to_string(),
        })?
        .insert_data(build);
    Ok(())
}

/// The trait's parameter type: an empty struct.
fn parameterless(spec: &DynamicTrait) -> TypeDesc {
    TypeDesc {
        name: spec.name.clone(),
        docs: spec.docs.clone(),
        shape: ShapeDesc::Struct {
            fields: Vec::new(),
            deny_unknown: true,
        },
        widget: None,
        category: None,
        default: Some(Value::Object(serde_json::Map::new())),
    }
}

/// Component `name`'s type default, as a value.
fn default_component(
    registry: &TypeRegistry,
    name: &str,
) -> Result<Box<dyn DynReflect>, TemplateError> {
    let registration = registry
        .get_by_name(name)
        .filter(|r| r.data::<ReflectComponent>().is_some())
        .ok_or_else(|| TemplateError::UnknownComponent {
            name: name.to_owned(),
            did_you_mean: None,
        })?;
    let info = registration.info();
    let default = match (&info.default, &info.shape) {
        (Some(default), _) => default.get(),
        (None, Shape::Unit) => Value::Null,
        (None, _) => {
            return Err(TemplateError::InvalidValue {
                what: name.to_owned(),
                reason: "has no default; script traits add components with their type defaults"
                    .into(),
            });
        }
    };
    registration
        .from_json(default)
        .map_err(|e| TemplateError::InvalidValue {
            what: name.to_owned(),
            reason: e.to_string(),
        })
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
