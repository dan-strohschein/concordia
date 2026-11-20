//! Components: any reflected type, plus the world's index of component types.

use std::any::TypeId;
use std::collections::HashMap;

use concordia_core::id::EntityId;
use concordia_reflect::wire::{WireColumn, WireError};
use concordia_reflect::{DynReflect, Reflect, RegistryError, TypeInfo, TypeRegistry};

use crate::error::EcsError;
use crate::storage::column::{Column, DynColumn, TypedColumn, downcast, downcast_dynamic};
use crate::storage::kind::StorageKind;
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
    pub(crate) kind: StorageKind,
    pub(crate) new_column: NewColumn,
}

/// How to make an empty column of a component.
#[derive(Clone, Copy)]
pub(crate) enum NewColumn {
    /// A Rust type's column.
    Typed(fn() -> Box<dyn Column>),
    /// A runtime-described type's column (its current version).
    Dynamic(&'static TypeInfo),
}

impl NewColumn {
    pub(crate) fn make(&self) -> Box<dyn Column> {
        match self {
            NewColumn::Typed(f) => f(),
            NewColumn::Dynamic(info) => Box::new(DynColumn::new(info)),
        }
    }
}

/// The component types a world has seen.
#[derive(Clone, Default)]
pub(crate) struct ComponentIndex {
    infos: Vec<ComponentInfo>,
    by_type: HashMap<TypeId, ComponentId>,
    by_name: HashMap<&'static str, ComponentId>,
}

impl ComponentIndex {
    /// The id of `T`, assigning one on first use (zero-sized types become tags).
    pub(crate) fn register<T: Component>(&mut self) -> Result<ComponentId, EcsError> {
        let kind = if size_of::<T>() == 0 {
            StorageKind::Tag
        } else {
            StorageKind::Dense
        };
        self.register_as::<T>(kind)
    }

    /// The id of `T`, stored as `kind` if it is new.
    pub(crate) fn register_as<T: Component>(
        &mut self,
        kind: StorageKind,
    ) -> Result<ComponentId, EcsError> {
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
            kind,
            new_column: NewColumn::Typed(|| Box::new(TypedColumn::<T>::default())),
        });
        self.by_type.insert(TypeId::of::<T>(), id);
        self.by_name.insert(name, id);
        Ok(id)
    }

    /// The id of the runtime-described type `info`, assigning one on first use (types with no
    /// data become tags). Another version of the same name must be migrated first.
    pub(crate) fn register_dynamic(
        &mut self,
        info: &'static TypeInfo,
    ) -> Result<ComponentId, EcsError> {
        if let Some(&id) = self.by_name.get(info.name) {
            return match self.infos[id.0].new_column {
                NewColumn::Dynamic(current) if std::ptr::eq(current, info) => Ok(id),
                NewColumn::Dynamic(_) => Err(EcsError::Dynamic {
                    component: info.name.to_owned(),
                    message: "a different version of this type is stored; migrate it first".into(),
                }),
                NewColumn::Typed(_) => Err(EcsError::NameCollision { name: info.name }),
            };
        }
        let layout = concordia_reflect::wire::layout(info).map_err(|e| EcsError::Dynamic {
            component: info.name.to_owned(),
            message: e.to_string(),
        })?;
        let kind = match layout.stride() {
            0 => StorageKind::Tag,
            _ => StorageKind::Dense,
        };
        let id = ComponentId(self.infos.len());
        self.infos.push(ComponentInfo {
            name: info.name,
            kind,
            new_column: NewColumn::Dynamic(info),
        });
        self.by_name.insert(info.name, id);
        Ok(id)
    }

    /// The type version new columns of a runtime-described component use.
    pub(crate) fn dynamic_version(&self, id: ComponentId) -> Option<&'static TypeInfo> {
        match self.infos[id.0].new_column {
            NewColumn::Dynamic(info) => Some(info),
            NewColumn::Typed(_) => None,
        }
    }

    /// Makes `info` the stored version of runtime-described component `id`.
    pub(crate) fn set_dynamic_version(&mut self, id: ComponentId, info: &'static TypeInfo) {
        debug_assert!(self.dynamic_version(id).is_some());
        self.infos[id.0].new_column = NewColumn::Dynamic(info);
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
type SignalDynFn = fn(&mut World, EntityId, Box<dyn DynReflect>) -> Result<(), EcsError>;
type FromWireFn = fn(&WireColumn, usize) -> Result<Box<dyn DynReflect>, WireError>;

/// Registry type data that lets the API, commands, scenes and scripts use components by name.
#[derive(Clone, Copy)]
pub struct ReflectComponent {
    insert: InsertDynFn,
    register_shared: fn(&mut World) -> Result<(), EcsError>,
    signal: SignalDynFn,
    from_wire: FromWireFn,
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
            register_shared: |world| world.register_shared::<T>(),
            signal: |world, id, value| {
                let value = downcast::<T>(value)?;
                live(world, id)?;
                world.signal(id, value);
                Ok(())
            },
            from_wire: |column, row| {
                let name = T::type_info().name;
                if column.info().name != name {
                    return Err(WireError::Mismatch(format!(
                        "a {} column holds no {name} values",
                        column.info().name
                    )));
                }
                Ok(Box::new(column.read::<T>(row)?))
            },
        }
    }

    /// Operations for runtime-described types (values are [`DynamicValue`]s).
    pub fn dynamic() -> Self {
        Self {
            insert: |world, id, value| {
                let value = downcast_dynamic(value)?;
                Ok(world
                    .insert_dynamic(id, value)?
                    .map(|old| Box::new(old) as Box<dyn DynReflect>))
            },
            register_shared: |_| {
                Err(EcsError::InvalidValue {
                    what: "storage kind of a script type",
                    expected: "dense or a tag",
                    value: "shared".into(),
                })
            },
            signal: |world, id, value| {
                let value = downcast_dynamic(value)?;
                live(world, id)?;
                world.signal_dynamic(id, value);
                Ok(())
            },
            from_wire: |column, row| Ok(Box::new(column.value(row)?)),
        }
    }

    /// Decodes row `row` of `column` as a value of this component's type (a `DynamicValue`
    /// for script types).
    pub fn from_wire(
        &self,
        column: &WireColumn,
        row: usize,
    ) -> Result<Box<dyn DynReflect>, WireError> {
        (self.from_wire)(column, row)
    }

    /// Raises `value` (a value of this component's type) as a signal on `entity`.
    pub fn signal(
        &self,
        world: &mut World,
        entity: EntityId,
        value: Box<dyn DynReflect>,
    ) -> Result<(), EcsError> {
        (self.signal)(world, entity, value)
    }

    /// Stores the component as shared in `world`, if the world hasn't used it yet
    /// (see [`World::register_shared`]).
    pub fn register_shared(&self, world: &mut World) -> Result<(), EcsError> {
        (self.register_shared)(world)
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

/// Registers a runtime-described type (see [`TypeRegistry::register_dynamic`]) as a component.
pub fn register_dynamic_component(
    registry: &mut TypeRegistry,
    desc: &concordia_reflect::TypeDesc,
) -> Result<&'static TypeInfo, RegistryError> {
    let registration = registry.register_dynamic(desc)?;
    registration.insert_data(ReflectComponent::dynamic());
    Ok(registration.info())
}

fn live(world: &World, id: EntityId) -> Result<(), EcsError> {
    match world.contains(id) {
        true => Ok(()),
        false => Err(EcsError::NoSuchEntity(id)),
    }
}
