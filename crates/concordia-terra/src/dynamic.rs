//! By-name component access and queries, for the Engine API, commands and scenes.

use concordia_core::id::EntityId;
use concordia_reflect::{DynReflect, JsonBuilder, TypeRegistration, TypeRegistry};
use serde_json::Value;

use crate::component::ReflectComponent;
use crate::error::EcsError;
use crate::world::World;

impl World {
    /// Borrows `id`'s component named `name`.
    pub fn get_dyn(&self, id: EntityId, name: &str) -> Result<&dyn DynReflect, EcsError> {
        let loc = self.entities.location(id)?;
        let missing = || EcsError::ComponentMissing {
            entity: id,
            component: name.to_owned(),
        };
        let cid = self.components.by_name(name).ok_or_else(missing)?;
        self.reflect_of(loc, cid).ok_or_else(missing)
    }

    /// `id`'s component named `name`, as JSON.
    pub fn get_json(&self, id: EntityId, name: &str) -> Result<Value, EcsError> {
        Ok(self.get_dyn(id, name)?.to_json()?)
    }

    /// True once this world has stored component `name` (it then holds a version of the type,
    /// even with no values left).
    pub fn stores(&self, name: &str) -> bool {
        self.components.by_name(name).is_some()
    }

    /// Inserts or replaces a dynamic component value; returns the previous value.
    pub fn insert_dyn(
        &mut self,
        id: EntityId,
        value: Box<dyn DynReflect>,
        registry: &TypeRegistry,
    ) -> Result<Option<Box<dyn DynReflect>>, EcsError> {
        if value.as_any().is::<concordia_reflect::DynamicValue>() {
            return self.insert_reflected(id, value, None);
        }
        let name = value.info().name;
        let registration = registry
            .get_by_type_id(value.as_any().type_id())
            .ok_or_else(|| EcsError::ComponentNotRegistered {
                name: name.to_owned(),
            })?;
        let reflect = *reflect_component(registration)?;
        self.insert_reflected(id, value, Some(&reflect))
    }

    /// Inserts a built value: script values directly, Rust values through `reflect`.
    fn insert_reflected(
        &mut self,
        id: EntityId,
        value: Box<dyn DynReflect>,
        reflect: Option<&ReflectComponent>,
    ) -> Result<Option<Box<dyn DynReflect>>, EcsError> {
        if value.as_any().is::<concordia_reflect::DynamicValue>() {
            let value = crate::storage::column::downcast_dynamic(value)?;
            let old = self.insert_dynamic(id, value)?;
            return Ok(old.map(|v| Box::new(v) as Box<dyn DynReflect>));
        }
        let name = value.info().name;
        reflect
            .ok_or_else(|| EcsError::NotAComponent {
                name: name.to_owned(),
            })?
            .insert(self, id, value)
    }

    /// Validates `json` as component `name` and inserts it; returns the previous value as JSON.
    pub fn set_json(
        &mut self,
        id: EntityId,
        name: &str,
        json: Value,
        registry: &TypeRegistry,
    ) -> Result<Option<Value>, EcsError> {
        let old = JsonComponent::of(registry, name)?.set(self, id, json)?;
        Ok(old.map(|v| v.to_json()).transpose()?)
    }

    /// Removes the component named `name`; `None` if the entity did not have it.
    pub fn remove_dyn(
        &mut self,
        id: EntityId,
        name: &str,
    ) -> Result<Option<Box<dyn DynReflect>>, EcsError> {
        self.entities.location(id)?;
        match self.components.by_name(name) {
            Some(cid) => self.remove_component(id, cid),
            None => Ok(None),
        }
    }
}

/// Builds component `name` from JSON, validating shape and ranges; never touches a world.
pub fn component_from_json(
    registry: &TypeRegistry,
    name: &str,
    json: Value,
) -> Result<Box<dyn DynReflect>, EcsError> {
    Ok(component_registration(registry, name)?.from_json(json)?)
}

fn component_registration<'r>(
    registry: &'r TypeRegistry,
    name: &str,
) -> Result<&'r TypeRegistration, EcsError> {
    let registration =
        registry
            .get_by_name(name)
            .ok_or_else(|| EcsError::ComponentNotRegistered {
                name: name.to_owned(),
            })?;
    reflect_component(registration)?;
    Ok(registration)
}

/// One component's JSON codec, copied out of the registry so play systems can write components by
/// name without it (tweens, 2.4.4).
#[derive(Debug, Clone, Copy)]
pub struct JsonComponent {
    builder: JsonBuilder,
    reflect: ReflectComponent,
}

impl JsonComponent {
    /// The codec of the component named `name`.
    pub fn of(registry: &TypeRegistry, name: &str) -> Result<Self, EcsError> {
        let registration = component_registration(registry, name)?;
        Ok(Self {
            builder: registration.json_builder(),
            reflect: *reflect_component(registration)?,
        })
    }

    /// The component's stable name.
    pub fn name(&self) -> &'static str {
        self.builder.info().name
    }

    /// Validates `json` and inserts it on `id`; returns the previous value.
    pub fn set(
        &self,
        world: &mut World,
        id: EntityId,
        json: Value,
    ) -> Result<Option<Box<dyn DynReflect>>, EcsError> {
        let value = self.builder.from_json(json)?;
        world.insert_reflected(id, value, Some(&self.reflect))
    }
}

fn reflect_component(registration: &TypeRegistration) -> Result<&ReflectComponent, EcsError> {
    registration
        .data::<ReflectComponent>()
        .ok_or_else(|| EcsError::NotAComponent {
            name: registration.info().name.to_owned(),
        })
}

/// A query by component names; returns matching entity ids sorted ascending.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DynamicQuery {
    /// Components every match must have.
    pub with: Vec<String>,
    /// Components no match may have.
    pub without: Vec<String>,
}

impl DynamicQuery {
    /// Runs the query; every name must be a registered component.
    pub fn run(&self, world: &World, registry: &TypeRegistry) -> Result<Vec<EntityId>, EcsError> {
        for name in self.with.iter().chain(&self.without) {
            component_registration(registry, name)?;
        }
        let Some(with) = self
            .with
            .iter()
            .map(|n| world.components.by_name(n))
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(Vec::new());
        };
        let without: Vec<_> = self
            .without
            .iter()
            .filter_map(|n| world.components.by_name(n))
            .collect();
        let mut ids: Vec<EntityId> = world
            .archetypes
            .list
            .iter()
            .filter(|a| with.iter().all(|&c| a.has(c)))
            .filter(|a| without.iter().all(|&c| !a.has(c)))
            .flat_map(|a| a.table.ids.iter().copied())
            .collect();
        ids.sort_unstable();
        Ok(ids)
    }
}
