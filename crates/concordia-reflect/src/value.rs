//! Dynamic values (ADR-0062): instances of runtime-described types, stored as canonical wire
//! bytes, that serialize, hash and validate exactly like a Rust type of the same shape.

use std::any::Any;

use serde_json::Value;

use crate::dynamic::DynReflect;
use crate::error::ReflectError;
use crate::info::TypeInfo;
use crate::info::{FieldInfo, Shape};
use crate::wire::{self, WireError};

/// A value of a dynamic type: its type and its canonical wire encoding.
#[derive(Clone)]
pub struct DynamicValue {
    info: &'static TypeInfo,
    bytes: Box<[u8]>,
}

impl DynamicValue {
    /// Validates `json` against `info` and encodes it.
    pub fn from_json(info: &'static TypeInfo, json: Value) -> Result<Self, ReflectError> {
        crate::validate::validate_json(&json, info)?;
        let bytes = wire::encode(&json, info).map_err(|e| ReflectError::Deserialize {
            type_name: info.name.to_owned(),
            message: e.to_string(),
        })?;
        Ok(Self {
            info,
            bytes: bytes.into_boxed_slice(),
        })
    }

    /// Validates one value's wire bytes (fixed part, then its arena).
    pub fn from_wire(info: &'static TypeInfo, bytes: &[u8]) -> Result<Self, WireError> {
        wire::validate(bytes, info)?;
        Ok(Self {
            info,
            bytes: bytes.into(),
        })
    }

    /// The value's type.
    pub fn info(&self) -> &'static TypeInfo {
        self.info
    }

    /// The canonical wire bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl PartialEq for DynamicValue {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.info, other.info) && self.bytes == other.bytes
    }
}

impl std::fmt::Debug for DynamicValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DynamicValue({}, {} bytes)",
            self.info.name,
            self.bytes.len()
        )
    }
}

impl serde::Serialize for DynamicValue {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        wire::serialize_value(&self.bytes, self.info, s)
    }
}

impl DynReflect for DynamicValue {
    fn info(&self) -> &'static TypeInfo {
        self.info
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }

    fn clone_dyn(&self) -> Box<dyn DynReflect> {
        Box::new(self.clone())
    }

    fn eq_dyn(&self, other: &dyn DynReflect) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_serialize(&self) -> &dyn erased_serde::Serialize {
        self
    }
}

/// What a migration changed, as JSON Pointers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationReport {
    /// Fields the new type added (filled with defaults).
    pub added: Vec<String>,
    /// Fields the new type dropped (their values are lost).
    pub removed: Vec<String>,
}

/// Converts `value` to the type `to` (a new version of its type): kept fields keep their values,
/// added fields take their defaults, removed fields are reported.
pub fn migrate(
    value: &DynamicValue,
    to: &'static TypeInfo,
) -> Result<(DynamicValue, MigrationReport), ReflectError> {
    let json = serde_json::to_value(value).map_err(|e| ReflectError::Serialize {
        type_name: value.info.name.to_owned(),
        message: e.to_string(),
    })?;
    let mut report = MigrationReport::default();
    let json = adapt(json, &crate::wire_shape(to).shape, "", &mut report)?;
    let migrated = DynamicValue::from_json(to, json)?;
    Ok((migrated, report))
}

/// Reshapes `json` for `shape`: drops fields it lacks, fills fields it adds.
fn adapt(
    json: Value,
    shape: &Shape,
    at: &str,
    report: &mut MigrationReport,
) -> Result<Value, ReflectError> {
    let (Shape::Struct { fields, .. }, Value::Object(mut old)) = (shape, json.clone()) else {
        return Ok(json);
    };
    let mut out = serde_json::Map::new();
    for field in fields {
        let pointer = format!("{at}/{}", field.name);
        match old.remove(field.name) {
            Some(v) => {
                let inner = crate::wire_shape(field.ty.info());
                out.insert(
                    field.name.to_owned(),
                    adapt(v, &inner.shape, &pointer, report)?,
                );
            }
            None => {
                out.insert(field.name.to_owned(), added(field, &pointer)?);
                report.added.push(pointer);
            }
        }
    }
    let mut removed: Vec<String> = old.keys().map(|k| format!("{at}/{k}")).collect();
    removed.sort();
    report.removed.extend(removed);
    Ok(Value::Object(out))
}

/// The value a field added by a migration starts with.
fn added(field: &FieldInfo, pointer: &str) -> Result<Value, ReflectError> {
    if let Some(default) = field.default {
        return Ok(default.get());
    }
    let info = crate::wire_shape(field.ty.info());
    if field.optional || matches!(info.shape, Shape::Option(_)) {
        return Ok(Value::Null);
    }
    if let Some(default) = info.default {
        return Ok(default.get());
    }
    let parent = pointer.rsplit_once('/').map_or("", |(p, _)| p);
    Err(ReflectError::MissingField {
        pointer: parent.to_owned(),
        field: field.name.to_owned(),
    })
}
