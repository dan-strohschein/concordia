//! The reflected type model: what a type looks like on the wire, plus its docs.

use std::fmt;

use crate::Reflect;
use crate::registry::{RegistryError, TypeRegistry};

/// Static description of a reflected type.
#[derive(Debug)]
pub struct TypeInfo {
    /// Stable name used in files, schemas and the API (e.g. `Transform`).
    pub name: &'static str,
    /// Rust type path (diagnostics only; not stable across compilers).
    pub rust_path: &'static str,
    /// Doc comment text (agent-facing).
    pub docs: &'static str,
    /// Wire shape.
    pub shape: Shape,
    /// Structural types (`Option`, `Vec`, maps, arrays, tuples) have no registry entry of their own.
    pub structural: bool,
}

impl TypeInfo {
    /// A named type with its own registry entry.
    pub fn named(
        name: &'static str,
        rust_path: &'static str,
        docs: &'static str,
        shape: Shape,
    ) -> Self {
        Self {
            name,
            rust_path,
            docs,
            shape,
            structural: false,
        }
    }

    /// A structural type that is always inlined in schemas.
    pub fn structural(name: &'static str, rust_path: &'static str, shape: Shape) -> Self {
        Self {
            name,
            rust_path,
            docs: "",
            shape,
            structural: true,
        }
    }

    /// True for user-defined data types (structs and enums), which schemas emit as `$defs`.
    pub fn is_definition(&self) -> bool {
        !self.structural
            && matches!(
                self.shape,
                Shape::Struct { .. }
                    | Shape::TupleStruct { .. }
                    | Shape::Newtype { .. }
                    | Shape::Unit
                    | Shape::Enum { .. }
            )
    }
}

/// How a value is laid out in serde's data model (JSON form noted per variant).
#[derive(Debug)]
pub enum Shape {
    /// A scalar.
    Primitive(Primitive),
    /// Named fields → JSON object.
    Struct {
        /// Serialized fields, in declaration order.
        fields: Vec<FieldInfo>,
        /// `#[serde(deny_unknown_fields)]`.
        deny_unknown: bool,
    },
    /// Positional fields (2+) → JSON array.
    TupleStruct {
        /// Field types.
        fields: Vec<TypeRef>,
    },
    /// One-field wrapper → the inner value.
    Newtype {
        /// Wrapped type.
        inner: TypeRef,
        /// `#[serde(transparent)]` (affects RON, not JSON).
        transparent: bool,
    },
    /// Unit struct → `null`.
    Unit,
    /// Externally tagged enum.
    Enum {
        /// Variants, in declaration order.
        variants: Vec<VariantInfo>,
    },
    /// `Option<T>` → `null` or the value.
    Option(TypeRef),
    /// `Vec<T>` → array.
    List(TypeRef),
    /// `BTreeMap<String, T>` → object.
    Map {
        /// Value type.
        value: TypeRef,
    },
    /// Fixed-length homogeneous array (`[T; N]`, glam vectors) → array of `len`.
    Array {
        /// Element type.
        elem: TypeRef,
        /// Exact length.
        len: usize,
    },
    /// Tuple → array.
    Tuple(Vec<TypeRef>),
    /// Any JSON value.
    Any,
}

/// Scalar kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primitive {
    /// `bool`.
    Bool,
    /// `i8`.
    I8,
    /// `i16`.
    I16,
    /// `i32`.
    I32,
    /// `i64`.
    I64,
    /// `u8`.
    U8,
    /// `u16`.
    U16,
    /// `u32`.
    U32,
    /// `u64`.
    U64,
    /// `f32`.
    F32,
    /// `f64`.
    F64,
    /// `String`.
    String,
    /// `char`.
    Char,
    /// A UUID string.
    Uuid,
    /// A string matching a regular expression (ECMA-262, as in JSON Schema).
    Pattern(&'static str),
}

impl Primitive {
    /// Integer bounds, if this is an integer type.
    pub fn int_bounds(self) -> Option<(i128, i128)> {
        let bounds = match self {
            Self::I8 => (i8::MIN.into(), i8::MAX.into()),
            Self::I16 => (i16::MIN.into(), i16::MAX.into()),
            Self::I32 => (i32::MIN.into(), i32::MAX.into()),
            Self::I64 => (i64::MIN.into(), i64::MAX.into()),
            Self::U8 => (0, u8::MAX.into()),
            Self::U16 => (0, u16::MAX.into()),
            Self::U32 => (0, u32::MAX.into()),
            Self::U64 => (0, u64::MAX.into()),
            _ => return None,
        };
        Some(bounds)
    }
}

/// A named field.
#[derive(Debug)]
pub struct FieldInfo {
    /// Field name on the wire.
    pub name: &'static str,
    /// Field type.
    pub ty: TypeRef,
    /// Doc comment text.
    pub docs: &'static str,
    /// `#[reflect(default = ..)]` as JSON (falls back to `null` if it cannot serialize).
    pub default: Option<fn() -> serde_json::Value>,
    /// `#[reflect(range(..))]`.
    pub range: Option<Range>,
    /// May be absent on input (`#[serde(default)]`, `skip_serializing_if`, or `Option`).
    pub optional: bool,
}

/// Inclusive numeric bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    /// Lower bound.
    pub min: f64,
    /// Upper bound.
    pub max: f64,
}

/// An enum variant.
#[derive(Debug)]
pub struct VariantInfo {
    /// Variant name on the wire.
    pub name: &'static str,
    /// Doc comment text.
    pub docs: &'static str,
    /// Payload shape.
    pub shape: VariantShape,
}

/// Payload of an enum variant.
#[derive(Debug)]
pub enum VariantShape {
    /// `"Name"`.
    Unit,
    /// `{"Name": value}`.
    Newtype(TypeRef),
    /// `{"Name": [a, b]}`.
    Tuple(Vec<TypeRef>),
    /// `{"Name": {fields}}`.
    Struct(Vec<FieldInfo>),
}

/// A lazy reference to another reflected type (cheap; safe for recursive types).
#[derive(Clone, Copy)]
pub struct TypeRef {
    info: fn() -> &'static TypeInfo,
    register: fn(&mut TypeRegistry) -> Result<(), RegistryError>,
}

impl TypeRef {
    /// Reference to `T`.
    pub fn of<T: Reflect>() -> Self {
        Self {
            info: T::type_info,
            register: |reg| reg.register::<T>().map(|_| ()),
        }
    }

    /// The referenced type's info.
    pub fn info(&self) -> &'static TypeInfo {
        (self.info)()
    }

    /// The referenced type's stable name.
    pub fn name(&self) -> &'static str {
        self.info().name
    }

    pub(crate) fn register(&self, reg: &mut TypeRegistry) -> Result<(), RegistryError> {
        (self.register)(reg)
    }
}

impl fmt::Debug for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypeRef({})", self.name())
    }
}

impl Shape {
    /// Every type this shape refers to directly.
    pub fn refs(&self) -> Vec<TypeRef> {
        match self {
            Self::Struct { fields, .. } => fields.iter().map(|f| f.ty).collect(),
            Self::TupleStruct { fields } | Self::Tuple(fields) => fields.clone(),
            Self::Newtype { inner: r, .. }
            | Self::Option(r)
            | Self::List(r)
            | Self::Map { value: r }
            | Self::Array { elem: r, .. } => vec![*r],
            Self::Enum { variants } => variants.iter().flat_map(|v| v.shape.refs()).collect(),
            Self::Primitive(_) | Self::Unit | Self::Any => Vec::new(),
        }
    }
}

impl VariantShape {
    fn refs(&self) -> Vec<TypeRef> {
        match self {
            Self::Unit => Vec::new(),
            Self::Newtype(r) => vec![*r],
            Self::Tuple(refs) => refs.clone(),
            Self::Struct(fields) => fields.iter().map(|f| f.ty).collect(),
        }
    }
}
