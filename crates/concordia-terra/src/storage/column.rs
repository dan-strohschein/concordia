//! A type-erased column: a `Vec<T>` behind a trait object (no unsafe code).

use std::any::Any;

use concordia_reflect::wire::{ColumnWriter, WireColumn, WireError};
use concordia_reflect::{DynReflect, DynamicValue, TypeInfo};

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
    /// The stored type.
    fn info(&self) -> &'static TypeInfo;
    /// Encodes `row` as row `out` of a wire column.
    fn encode_row(&self, row: usize, w: &mut ColumnWriter, out: usize) -> Result<(), WireError>;
    /// Decodes rows `srcs` of a wire column of this type, without storing them yet.
    fn decode(&self, column: &WireColumn, srcs: &[usize])
    -> Result<Box<dyn Any + Send>, WireError>;
    /// Stores values from [`Column::decode`] at `rows`.
    fn assign(&mut self, rows: &[usize], decoded: Box<dyn Any + Send>);
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

    fn info(&self) -> &'static TypeInfo {
        T::type_info()
    }

    fn encode_row(&self, row: usize, w: &mut ColumnWriter, out: usize) -> Result<(), WireError> {
        w.write(out, &self.0[row])
    }

    fn decode(
        &self,
        column: &WireColumn,
        srcs: &[usize],
    ) -> Result<Box<dyn Any + Send>, WireError> {
        let values: Vec<T> = srcs
            .iter()
            .map(|&s| column.read::<T>(s))
            .collect::<Result<_, _>>()?;
        Ok(Box::new(values))
    }

    fn assign(&mut self, rows: &[usize], decoded: Box<dyn Any + Send>) {
        let values = decoded
            .downcast::<Vec<T>>()
            .expect("decoded by this column type");
        for (&row, value) in rows.iter().zip(*values) {
            self.0[row] = value;
        }
    }
}

/// Column storage for a runtime-described type: its values with their (current) type.
#[derive(Clone)]
pub(crate) struct DynColumn {
    info: &'static TypeInfo,
    pub(crate) rows: Vec<DynamicValue>,
}

impl DynColumn {
    pub(crate) fn new(info: &'static TypeInfo) -> Self {
        Self {
            info,
            rows: Vec::new(),
        }
    }
}

impl Column for DynColumn {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn len(&self) -> usize {
        self.rows.len()
    }

    fn clone_box(&self) -> Box<dyn Column> {
        Box::new(self.clone())
    }

    fn swap_remove_drop(&mut self, row: usize) {
        self.rows.swap_remove(row);
    }

    fn swap_remove_into(&mut self, row: usize, dst: &mut dyn Column) {
        let value = self.rows.swap_remove(row);
        dynamic_mut(dst).push(value);
    }

    fn swap_remove_boxed(&mut self, row: usize) -> Box<dyn DynReflect> {
        Box::new(self.rows.swap_remove(row))
    }

    fn get_reflect(&self, row: usize) -> &dyn DynReflect {
        &self.rows[row]
    }

    fn info(&self) -> &'static TypeInfo {
        self.info
    }

    fn encode_row(&self, row: usize, w: &mut ColumnWriter, out: usize) -> Result<(), WireError> {
        w.write_value(out, &self.rows[row])
    }

    fn decode(
        &self,
        column: &WireColumn,
        srcs: &[usize],
    ) -> Result<Box<dyn Any + Send>, WireError> {
        if !std::ptr::eq(column.info(), self.info) {
            return Err(WireError::Mismatch(format!(
                "a {} column cannot hold another version of {}",
                self.info.name,
                column.info().name
            )));
        }
        let values: Vec<DynamicValue> = srcs
            .iter()
            .map(|&s| column.value(s))
            .collect::<Result<_, _>>()?;
        Ok(Box::new(values))
    }

    fn assign(&mut self, rows: &[usize], decoded: Box<dyn Any + Send>) {
        let values = decoded
            .downcast::<Vec<DynamicValue>>()
            .expect("decoded by this column type");
        for (&row, value) in rows.iter().zip(*values) {
            self.rows[row] = value;
        }
    }
}

/// The values behind a column known to hold a runtime-described type.
pub(crate) fn dynamic_mut(column: &mut dyn Column) -> &mut Vec<DynamicValue> {
    &mut column
        .as_any_mut()
        .downcast_mut::<DynColumn>()
        .expect("column type matches its component id")
        .rows
}

/// Unboxes a dynamic value of a runtime-described type.
pub(crate) fn downcast_dynamic(value: Box<dyn DynReflect>) -> Result<DynamicValue, EcsError> {
    let found = value.info().name;
    value
        .into_any()
        .downcast::<DynamicValue>()
        .map(|v| *v)
        .map_err(|_| EcsError::TypeMismatch {
            expected: "a script value",
            found,
        })
}

/// Encodes one reflected value (a shared value, or a signal payload) as row `out`.
pub(crate) fn encode_reflect(
    value: &dyn DynReflect,
    w: &mut ColumnWriter,
    out: usize,
) -> Result<(), WireError> {
    match value.as_any().downcast_ref::<DynamicValue>() {
        Some(dynamic) => w.write_value(out, dynamic),
        None => w.write(out, value.as_serialize()),
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
