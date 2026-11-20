//! Shared values (ADR-0069): one stored value per distinct (component, value), interned by the
//! value's canonical hash. Archetypes refer to them by index; tags are shared values of a
//! zero-sized type.

use std::any::Any;
use std::collections::HashMap;

use concordia_reflect::{DynReflect, canonical_hash};

use crate::component::{Component, ComponentId};
use crate::error::EcsError;

/// One component type's stored values.
pub(crate) trait SharedValues: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn clone_box(&self) -> Box<dyn SharedValues>;
    fn len(&self) -> usize;
    fn get_reflect(&self, index: usize) -> &dyn DynReflect;
}

impl Clone for Box<dyn SharedValues> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[derive(Clone)]
struct Typed<T>(Vec<T>);

impl<T: DynReflect + Clone> SharedValues for Typed<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn SharedValues> {
        Box::new(self.clone())
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn get_reflect(&self, index: usize) -> &dyn DynReflect {
        &self.0[index]
    }
}

/// Every shared value of a world.
#[derive(Clone, Default)]
pub(crate) struct SharedStore {
    by_component: HashMap<ComponentId, Box<dyn SharedValues>>,
    index: HashMap<(ComponentId, [u8; 32]), u32>,
}

impl SharedStore {
    /// The index of `value`, storing it if it is new.
    pub(crate) fn intern<T: Component>(
        &mut self,
        component: ComponentId,
        value: T,
    ) -> Result<u32, EcsError> {
        self.intern_value(component, value, size_of::<T>() == 0)
    }

    /// The index of a runtime-described value (`tag`: the type has no data).
    pub(crate) fn intern_dynamic(
        &mut self,
        component: ComponentId,
        value: concordia_reflect::DynamicValue,
        tag: bool,
    ) -> Result<u32, EcsError> {
        self.intern_value(component, value, tag)
    }

    fn intern_value<T: DynReflect + Clone>(
        &mut self,
        component: ComponentId,
        value: T,
        tag: bool,
    ) -> Result<u32, EcsError> {
        // A tag has exactly one value: index 0 once stored.
        if tag && self.by_component.contains_key(&component) {
            return Ok(0);
        }
        let mut h = blake3::Hasher::new();
        canonical_hash(&value, &mut h)?;
        let key = (component, *h.finalize().as_bytes());
        if let Some(&existing) = self.index.get(&key) {
            return Ok(existing);
        }
        let values = self
            .by_component
            .entry(component)
            .or_insert_with(|| Box::new(Typed::<T>(Vec::new())));
        let typed = values
            .as_any_mut()
            .downcast_mut::<Typed<T>>()
            .expect("shared values match their component id");
        let index = u32::try_from(typed.0.len()).map_err(|_| EcsError::InvalidValue {
            what: "shared values per component",
            expected: "fewer than 2^32",
            value: typed.0.len().to_string(),
        })?;
        typed.0.push(value);
        self.index.insert(key, index);
        Ok(index)
    }

    /// The stored value `index` of `component`, typed.
    pub(crate) fn get<T: Component>(&self, component: ComponentId, index: u32) -> Option<&T> {
        self.get_value(component, index)
    }

    /// The stored value `index` of a runtime-described `component`.
    pub(crate) fn get_dynamic(
        &self,
        component: ComponentId,
        index: u32,
    ) -> Option<&concordia_reflect::DynamicValue> {
        self.get_value(component, index)
    }

    fn get_value<T: 'static>(&self, component: ComponentId, index: u32) -> Option<&T> {
        self.by_component
            .get(&component)?
            .as_any()
            .downcast_ref::<Typed<T>>()?
            .0
            .get(index as usize)
    }

    /// The stored values of a runtime-described `component`, in index order.
    pub(crate) fn dynamic_values(
        &self,
        component: ComponentId,
    ) -> Option<&[concordia_reflect::DynamicValue]> {
        self.by_component
            .get(&component)?
            .as_any()
            .downcast_ref::<Typed<concordia_reflect::DynamicValue>>()
            .map(|t| t.0.as_slice())
    }

    /// Replaces the stored values of a runtime-described `component` (same count and order;
    /// for a new version of the type, which hashes the same way).
    pub(crate) fn set_dynamic_values(
        &mut self,
        component: ComponentId,
        values: Vec<concordia_reflect::DynamicValue>,
    ) {
        if let Some(typed) = self.by_component.get_mut(&component).and_then(|v| {
            v.as_any_mut()
                .downcast_mut::<Typed<concordia_reflect::DynamicValue>>()
        }) {
            debug_assert_eq!(typed.0.len(), values.len());
            typed.0 = values;
        }
    }

    /// The stored value `index` of `component`, reflected.
    pub(crate) fn get_reflect(&self, component: ComponentId, index: u32) -> &dyn DynReflect {
        self.by_component[&component].get_reflect(index as usize)
    }

    /// Stored values across all components.
    pub(crate) fn len(&self) -> usize {
        self.by_component.values().map(|v| v.len()).sum()
    }
}
