//! Components: any reflected type, plus the world's index of component types.

use std::any::TypeId;
use std::collections::HashMap;

use concordia_core::id::EntityId;
use concordia_reflect::{DynReflect, Reflect, RegistryError, TypeRegistry};

use crate::error::EcsError;
use crate::storage::column::{Column, TypedColumn, downcast};
use crate::world::World;

/// A type that can be stored on entities. Every [`Reflect`] type is a component.
pub trait Component: Reflect {}

impl<T: Reflect> Component for T {}

/// Dense per-world component index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ComponentId(usize);

#[derive(Clone)]
pub(crate) struct ComponentInfo {
    pub(crate) name: &'static str,
    pub(crate) new_column: fn() -> Box<dyn Column>,
}

/// The component types a world has seen.
#[derive(Clone, Default)]
pub(crate) struct ComponentIndex {
    infos: Vec<ComponentInfo>,
    by_type: HashMap<TypeId, ComponentId>,
    by_name: HashMap<&'static str, ComponentId>,
}

impl ComponentIndex {
    /// The id of `T`, assigning one on first use.
    pub(crate) fn register<T: Component>(&mut self) -> Result<ComponentId, EcsError> {
        if let Some(&id) = self.by_type.get(&TypeId::of::<T>()) {
            return Ok(id);
        }
        let name = T::type_info().name;
        if self.by_name.contains_key(name) {
            return Err(EcsError::NameCollision { name });
        }
        let id = ComponentId(self.infos.len());
        self.infos.push(ComponentInfo {
            name,
            new_column: || Box::new(TypedColumn::<T>::default()),
        });
        self.by_type.insert(TypeId::of::<T>(), id);
        self.by_name.insert(name, id);
        Ok(id)
    }

    pub(crate) fn id_of(&self, type_id: TypeId) -> Option<ComponentId> {
        self.by_type.get(&type_id).copied()
    }

    pub(crate) fn by_name(&self, name: &str) -> Option<ComponentId> {
        self.by_name.get(name).copied()
    }

    pub(crate) fn info(&self, id: ComponentId) -> &ComponentInfo {
        &self.infos[id.0]
    }
}

type InsertDynFn =
    fn(&mut World, EntityId, Box<dyn DynReflect>) -> Result<Option<Box<dyn DynReflect>>, EcsError>;

/// Registry type data that lets the API, commands and scenes insert components by name.
#[derive(Clone, Copy)]
pub struct ReflectComponent {
    insert: InsertDynFn,
}

impl ReflectComponent {
    fn of<T: Component>() -> Self {
        Self {
            insert: |world, id, value| {
                let value = downcast::<T>(value)?;
                Ok(world
                    .insert(id, value)?
                    .map(|old| Box::new(old) as Box<dyn DynReflect>))
            },
        }
    }

    /// Inserts (or replaces) `value` on `entity`, returning the previous value.
    pub fn insert(
        &self,
        world: &mut World,
        entity: EntityId,
        value: Box<dyn DynReflect>,
    ) -> Result<Option<Box<dyn DynReflect>>, EcsError> {
        (self.insert)(world, entity, value)
    }
}

impl std::fmt::Debug for ReflectComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReflectComponent")
    }
}

/// Registers `T` in `registry` and marks it as a component.
pub fn register_component<T: Component>(registry: &mut TypeRegistry) -> Result<(), RegistryError> {
    registry
        .register::<T>()?
        .insert_data(ReflectComponent::of::<T>());
    Ok(())
}
