//! Type-erased reflected values.

use std::any::Any;

use crate::error::ReflectError;
use crate::info::TypeInfo;
use crate::reflect::Reflect;

/// Object-safe view of any [`Reflect`] value.
pub trait DynReflect: Any + Send + Sync {
    /// The value's type info.
    fn info(&self) -> &'static TypeInfo;
    /// Upcast for downcasting.
    fn as_any(&self) -> &dyn Any;
    /// Mutable upcast for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Boxed clone.
    fn clone_dyn(&self) -> Box<dyn DynReflect>;
    /// Equality with another value of possibly different type.
    fn eq_dyn(&self, other: &dyn DynReflect) -> bool;
    /// Erased serde view.
    fn as_serialize(&self) -> &dyn erased_serde::Serialize;
}

impl<T: Reflect> DynReflect for T {
    fn info(&self) -> &'static TypeInfo {
        T::type_info()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn clone_dyn(&self) -> Box<dyn DynReflect> {
        Box::new(self.clone())
    }

    fn eq_dyn(&self, other: &dyn DynReflect) -> bool {
        other.as_any().downcast_ref::<T>() == Some(self)
    }

    fn as_serialize(&self) -> &dyn erased_serde::Serialize {
        self
    }
}

impl dyn DynReflect {
    /// Serializes to JSON.
    pub fn to_json(&self) -> Result<serde_json::Value, ReflectError> {
        serde_json::to_value(self.as_serialize()).map_err(|e| ReflectError::Serialize {
            type_name: self.info().name.to_owned(),
            message: e.to_string(),
        })
    }
}

impl Clone for Box<dyn DynReflect> {
    fn clone(&self) -> Self {
        self.as_ref().clone_dyn()
    }
}

impl std::fmt::Debug for dyn DynReflect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.to_json() {
            Ok(json) => write!(f, "{}({json})", self.info().name),
            Err(_) => write!(f, "{}(<unserializable>)", self.info().name),
        }
    }
}
