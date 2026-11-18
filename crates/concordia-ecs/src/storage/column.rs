//! A type-erased column: a `Vec<T>` behind a trait object (no unsafe code).

use std::any::Any;

use concordia_reflect::DynReflect;

use crate::component::Component;
use crate::error::EcsError;

/// Operations on one component column, independent of its element type.
pub(crate) trait Column: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn len(&self) -> usize;
    fn clone_box(&self) -> Box<dyn Column>;
    /// Removes `row` by swapping in the last element, dropping the value.
    fn swap_remove_drop(&mut self, row: usize);
    /// Removes `row` by swapping in the last element, pushing the value onto `dst` (same type).
    fn swap_remove_into(&mut self, row: usize, dst: &mut dyn Column);
    /// Removes `row` by swapping in the last element, returning the value boxed.
    fn swap_remove_boxed(&mut self, row: usize) -> Box<dyn DynReflect>;
    /// Reflected view of `row`.
    fn get_reflect(&self, row: usize) -> &dyn DynReflect;
}

impl Clone for Box<dyn Column> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Column storage for component `T`.
#[derive(Clone)]
pub(crate) struct TypedColumn<T>(pub(crate) Vec<T>);

impl<T> Default for TypedColumn<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T: Component> Column for TypedColumn<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn clone_box(&self) -> Box<dyn Column> {
        Box::new(self.clone())
    }

    fn swap_remove_drop(&mut self, row: usize) {
        self.0.swap_remove(row);
    }

    fn swap_remove_into(&mut self, row: usize, dst: &mut dyn Column) {
        let value = self.0.swap_remove(row);
        typed_mut::<T>(dst).push(value);
    }

    fn swap_remove_boxed(&mut self, row: usize) -> Box<dyn DynReflect> {
        Box::new(self.0.swap_remove(row))
    }

    fn get_reflect(&self, row: usize) -> &dyn DynReflect {
        &self.0[row]
    }
}

/// The typed vector behind a column known to hold `T`.
pub(crate) fn typed<T: Component>(column: &dyn Column) -> &Vec<T> {
    &column
        .as_any()
        .downcast_ref::<TypedColumn<T>>()
        .expect("column type matches its component id")
        .0
}

/// Mutable typed vector behind a column known to hold `T`.
pub(crate) fn typed_mut<T: Component>(column: &mut dyn Column) -> &mut Vec<T> {
    &mut column
        .as_any_mut()
        .downcast_mut::<TypedColumn<T>>()
        .expect("column type matches its component id")
        .0
}

/// Unboxes a dynamic value into `T`.
pub(crate) fn downcast<T: Component>(value: Box<dyn DynReflect>) -> Result<T, EcsError> {
    let found = value.info().name;
    value
        .into_any()
        .downcast::<T>()
        .map(|v| *v)
        .map_err(|_| EcsError::TypeMismatch {
            expected: T::type_info().name,
            found,
        })
}
