//! Runtime registry of reflected types, looked up by stable name or `TypeId`.

use std::any::{Any, TypeId};
use std::collections::{BTreeMap, HashMap};

use serde_json::Value;

use crate::dynamic::DynReflect;
use crate::error::ReflectError;
use crate::info::TypeInfo;
use crate::reflect::Reflect;
use crate::value::DynamicValue;
use crate::{schema, validate};

/// Errors from registering types.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// Two different Rust types claim the same stable name.
    #[error(
        "type name {name:?} is used by both {existing} and {new}; rename one with #[reflect(name = \"...\")]"
    )]
    NameCollision {
        /// The contested name.
        name: &'static str,
        /// Rust path of the registered type.
        existing: &'static str,
        /// Rust path of the rejected type.
        new: &'static str,
    },
    /// A dynamic type refers to a name that is not registered.
    #[error("{referenced_by} refers to {name:?}, which is not a registered type")]
    UnknownType {
        /// The missing name.
        name: String,
        /// The type that refers to it.
        referenced_by: String,
    },
    /// A dynamic type's description is invalid.
    #[error("invalid type description {name:?}: {message}")]
    InvalidDesc {
        /// The described type.
        name: String,
        /// What is wrong.
        message: String,
    },
}

type FromJsonFn = fn(Value) -> Result<Box<dyn DynReflect>, ReflectError>;
type DeserializeFn =
    fn(&mut dyn erased_serde::Deserializer<'_>) -> Result<Box<dyn DynReflect>, erased_serde::Error>;

/// How a registered type builds values.
enum Builds {
    /// A Rust type.
    Static {
        from_json: FromJsonFn,
        deserialize: DeserializeFn,
    },
    /// A runtime-described type; values are [`DynamicValue`]s.
    Dynamic,
}

/// Everything the engine knows about one registered type.
pub struct TypeRegistration {
    info: &'static TypeInfo,
    type_id: TypeId,
    builds: Builds,
    data: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl TypeRegistration {
    fn of<T: Reflect>() -> Self {
        Self {
            info: T::type_info(),
            type_id: TypeId::of::<T>(),
            builds: Builds::Static {
                from_json: from_json_impl::<T>,
                deserialize: |d| {
                    erased_serde::deserialize::<T>(d).map(|v| Box::new(v) as Box<dyn DynReflect>)
                },
            },
            data: HashMap::new(),
        }
    }

    fn dynamic(info: &'static TypeInfo) -> Self {
        Self {
            info,
            type_id: TypeId::of::<DynamicValue>(),
            builds: Builds::Dynamic,
            data: HashMap::new(),
        }
    }

    /// The type's info.
    pub fn info(&self) -> &'static TypeInfo {
        self.info
    }

    /// True for a type described at runtime (see [`TypeRegistry::register_dynamic`]).
    pub fn is_dynamic(&self) -> bool {
        matches!(self.builds, Builds::Dynamic)
    }

    /// Builds a value from any serde deserializer (used by scene loading).
    pub fn deserialize(
        &self,
        deserializer: &mut dyn erased_serde::Deserializer<'_>,
    ) -> Result<Box<dyn DynReflect>, erased_serde::Error> {
        match self.builds {
            Builds::Static { deserialize, .. } => deserialize(deserializer),
            Builds::Dynamic => {
                let json: Value = erased_serde::deserialize(deserializer)?;
                self.from_json(json)
                    .map_err(<erased_serde::Error as serde::de::Error>::custom)
            }
        }
    }

    /// The Rust `TypeId` (that of [`DynamicValue`] for every dynamic type).
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Validates `value` against the type's shape, then deserializes it.
    pub fn from_json(&self, value: Value) -> Result<Box<dyn DynReflect>, ReflectError> {
        match self.builds {
            Builds::Static { from_json, .. } => from_json(value),
            Builds::Dynamic => Ok(Box::new(DynamicValue::from_json(self.info, value)?)),
        }
    }

    /// Attaches extension data (e.g. ECS component operations), replacing any previous `D`.
    pub fn insert_data<D: Any + Send + Sync>(&mut self, data: D) {
        self.data.insert(TypeId::of::<D>(), Box::new(data));
    }

    /// Extension data of type `D`, if attached.
    pub fn data<D: Any>(&self) -> Option<&D> {
        self.data.get(&TypeId::of::<D>())?.downcast_ref()
    }
}

impl std::fmt::Debug for TypeRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeRegistration")
            .field("name", &self.info.name)
            .field("data_slots", &self.data.len())
            .finish()
    }
}

fn from_json_impl<T: Reflect>(value: Value) -> Result<Box<dyn DynReflect>, ReflectError> {
    validate::validate_json(&value, T::type_info())?;
    serde_json::from_value::<T>(value)
        .map(|v| Box::new(v) as Box<dyn DynReflect>)
        .map_err(|e| ReflectError::Deserialize {
            type_name: T::type_info().name.to_owned(),
            message: e.to_string(),
        })
}

/// All registered types.
#[derive(Debug, Default)]
pub struct TypeRegistry {
    entries: Vec<TypeRegistration>,
    by_name: BTreeMap<&'static str, usize>,
    by_id: HashMap<TypeId, usize>,
}

impl TypeRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `T` and every named type it refers to. Idempotent.
    pub fn register<T: Reflect>(&mut self) -> Result<&mut TypeRegistration, RegistryError> {
        let index = match self.by_id.get(&TypeId::of::<T>()) {
            Some(&index) => index,
            None => self.insert::<T>()?,
        };
        Ok(&mut self.entries[index])
    }

    fn insert<T: Reflect>(&mut self) -> Result<usize, RegistryError> {
        let info = T::type_info();
        if !info.structural {
            self.claim_name::<T>(info)?;
        }
        // Mark as seen before walking fields so recursive types terminate.
        let index = self.entries.len();
        self.entries.push(TypeRegistration::of::<T>());
        self.by_id.insert(TypeId::of::<T>(), index);
        if !info.structural {
            self.by_name.insert(info.name, index);
        }
        for dependency in info.shape.refs() {
            dependency.register(self)?;
        }
        Ok(index)
    }

    fn claim_name<T: Reflect>(&self, info: &'static TypeInfo) -> Result<(), RegistryError> {
        match self.by_name.get(info.name) {
            Some(&existing) => Err(RegistryError::NameCollision {
                name: info.name,
                existing: self.entries[existing].info.rust_path,
                new: std::any::type_name::<T>(),
            }),
            None => Ok(()),
        }
    }

    /// Registers (or replaces) a type described at runtime; its values are
    /// [`DynamicValue`](crate::DynamicValue)s. A static type's name cannot be taken.
    pub fn register_dynamic(
        &mut self,
        desc: &crate::TypeDesc,
    ) -> Result<&mut TypeRegistration, RegistryError> {
        let existing = self.by_name.get(desc.name.as_str()).copied();
        if let Some(index) = existing
            && !self.entries[index].is_dynamic()
        {
            let info = self.entries[index].info;
            return Err(RegistryError::NameCollision {
                name: info.name,
                existing: info.rust_path,
                new: crate::desc::DYNAMIC_PATH,
            });
        }
        let info = desc.to_info(self)?;
        let index = match existing {
            Some(index) => {
                // A new version (e.g. a script reload): keep attached data such as component ops.
                let data = std::mem::take(&mut self.entries[index].data);
                self.entries[index] = TypeRegistration::dynamic(info);
                self.entries[index].data = data;
                index
            }
            None => {
                self.entries.push(TypeRegistration::dynamic(info));
                self.by_name.insert(info.name, self.entries.len() - 1);
                self.entries.len() - 1
            }
        };
        Ok(&mut self.entries[index])
    }

    /// Lookup by Rust type.
    pub fn get<T: 'static>(&self) -> Option<&TypeRegistration> {
        self.by_id
            .get(&TypeId::of::<T>())
            .map(|&i| &self.entries[i])
    }

    /// Mutable lookup by Rust type (to attach type data).
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut TypeRegistration> {
        let index = *self.by_id.get(&TypeId::of::<T>())?;
        Some(&mut self.entries[index])
    }

    /// Lookup by `TypeId`.
    pub fn get_by_type_id(&self, id: TypeId) -> Option<&TypeRegistration> {
        self.by_id.get(&id).map(|&i| &self.entries[i])
    }

    /// Lookup by stable name.
    pub fn get_by_name(&self, name: &str) -> Option<&TypeRegistration> {
        self.by_name.get(name).map(|&i| &self.entries[i])
    }

    /// Stable names of named types, sorted.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.by_name.keys().copied()
    }

    /// Builds a value of the type named `name` from JSON.
    pub fn from_json(&self, name: &str, value: Value) -> Result<Box<dyn DynReflect>, ReflectError> {
        self.get_by_name(name)
            .ok_or_else(|| ReflectError::UnknownType {
                name: name.to_owned(),
            })?
            .from_json(value)
    }

    /// JSON Schema (2020-12) for the type named `name`.
    pub fn schema_for(&self, name: &str) -> Option<Value> {
        self.get_by_name(name).map(|r| schema::schema_for(r.info))
    }

    /// Sorted index of every named type: name, kind and first doc line.
    pub fn schema_index(&self) -> Value {
        let types: Vec<Value> = self
            .by_name
            .values()
            .map(|&i| schema::index_entry(self.entries[i].info))
            .collect();
        serde_json::json!({ "title": "Concordia type index", "types": types })
    }
}
