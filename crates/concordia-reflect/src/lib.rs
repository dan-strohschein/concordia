//! Concordia reflection: every engine type describes its shape, docs, defaults and
//! ranges, so tools and agents can discover, validate, schema-export and hash it.
//!
//! ```
//! use concordia_reflect::{Reflect, TypeRegistry};
//! use serde::{Deserialize, Serialize};
//!
//! /// A health pool.
//! #[derive(Clone, PartialEq, Serialize, Deserialize, Reflect)]
//! struct Health {
//!     /// Current hit points.
//!     #[reflect(range(min = 0.0, max = 100.0))]
//!     hp: f32,
//! }
//!
//! let mut registry = TypeRegistry::new();
//! registry.register::<Health>()?;
//! let value = registry.from_json("Health", serde_json::json!({"hp": 5.0}))?;
//! assert_eq!(value.info().docs, "A health pool.");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod desc;
mod dynamic;
mod error;
mod example;
mod finite;
mod hash;
mod impls;
mod info;
mod reflect;
mod registry;
mod schema;
mod validate;
mod value;
pub mod wire;

pub use concordia_reflect_derive::Reflect;
pub use desc::{
    FieldDesc, PrimitiveDesc, RangeDesc, ShapeDesc, TypeDesc, TypeRefDesc, VariantDesc,
    VariantShapeDesc,
};
pub use dynamic::DynReflect;
pub use error::ReflectError;
pub use example::example_json;
pub use finite::find_non_finite;
pub use hash::canonical_hash;
pub use info::{
    DefaultValue, FieldInfo, Primitive, Range, Shape, TypeInfo, TypeRef, VariantInfo, VariantShape,
};
pub use reflect::Reflect;
pub use registry::{RegistryError, TypeRegistration, TypeRegistry};
pub use schema::{DRAFT, index_entry, schema_for};
pub use validate::validate_json;
pub use value::{DynamicValue, MigrationReport, migrate};

/// The type behind newtype wrappers (how it is laid out and migrated).
pub(crate) fn wire_shape(info: &'static TypeInfo) -> &'static TypeInfo {
    let mut info = info;
    while let Shape::Newtype { inner, .. } = &info.shape {
        info = inner.info();
    }
    info
}

/// Support code for `#[derive(Reflect)]`; not a public API.
#[doc(hidden)]
pub mod __private {
    /// Serializes a field default for schemas (`null` if it cannot serialize).
    pub fn default_json<T: serde::Serialize>(value: T) -> serde_json::Value {
        serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
    }
}
