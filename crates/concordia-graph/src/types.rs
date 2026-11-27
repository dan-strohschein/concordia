//! Pin types (M4 4.9, H6): what flows along a data link.

use std::fmt;

use concordia_reflect::{Shape, TypeRef};
use serde::{Serialize, Serializer};

/// A data pin's type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PinType {
    /// `bool`.
    Bool,
    /// `i32`.
    I32,
    /// `i64`.
    I64,
    /// `f32`.
    F32,
    /// `Vec2`.
    Vec2,
    /// `Vec3`.
    Vec3,
    /// `Vec4`.
    Vec4,
    /// `Quat`.
    Quat,
    /// An entity.
    EntityId,
    /// An asset reference (untyped: reflection has no `AssetRef<T>`).
    AssetRef,
    /// `String`.
    String,
    /// A reflected struct or enum, by name.
    Named(String),
    /// `List<T>`.
    List(Box<PinType>),
    /// `Option<T>`.
    Option(Box<PinType>),
    /// A type variable resolved per node at check time.
    Generic(GenericKind),
}

/// What a generic pin accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GenericKind {
    /// `i32`, `i64`, `f32`, `Vec2`–`Vec4` (arithmetic).
    Numeric,
    /// `i32`, `i64`, `f32` (comparisons).
    Scalar,
}

impl GenericKind {
    /// Whether `ty` satisfies the constraint.
    pub fn admits(self, ty: &PinType) -> bool {
        let scalar = matches!(ty, PinType::I32 | PinType::I64 | PinType::F32);
        match self {
            Self::Scalar => scalar,
            Self::Numeric => scalar || matches!(ty, PinType::Vec2 | PinType::Vec3 | PinType::Vec4),
        }
    }
}

impl PinType {
    /// Parses a type name (`f32`, `List<EntityId>`, `Option<Vec3>`, a reflected name).
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let wrapped = |prefix: &str| {
            text.strip_prefix(prefix)
                .and_then(|rest| rest.strip_suffix('>'))
        };
        if let Some(inner) = wrapped("List<") {
            return Ok(Self::List(Box::new(Self::parse(inner)?)));
        }
        if let Some(inner) = wrapped("Option<") {
            return Ok(Self::Option(Box::new(Self::parse(inner)?)));
        }
        let simple = match text {
            "bool" => Self::Bool,
            "i32" => Self::I32,
            "i64" => Self::I64,
            "f32" => Self::F32,
            "Vec2" => Self::Vec2,
            "Vec3" => Self::Vec3,
            "Vec4" => Self::Vec4,
            "Quat" => Self::Quat,
            "EntityId" => Self::EntityId,
            "AssetRef" => Self::AssetRef,
            "String" => Self::String,
            "Numeric" => Self::Generic(GenericKind::Numeric),
            "Scalar" => Self::Generic(GenericKind::Scalar),
            name if is_identifier(name) => Self::Named(name.to_owned()),
            other => return Err(format!("{other:?} is not a type")),
        };
        Ok(simple)
    }

    /// The pin type of a reflected type.
    pub fn of(ty: &TypeRef) -> Self {
        let info = ty.info();
        match &info.shape {
            Shape::Option(inner) if info.structural => Self::Option(Box::new(Self::of(inner))),
            Shape::List(inner) if info.structural => Self::List(Box::new(Self::of(inner))),
            _ => Self::parse(info.name).unwrap_or_else(|_| Self::Named(info.name.to_owned())),
        }
    }
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

impl fmt::Display for PinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool => f.write_str("bool"),
            Self::I32 => f.write_str("i32"),
            Self::I64 => f.write_str("i64"),
            Self::F32 => f.write_str("f32"),
            Self::Vec2 => f.write_str("Vec2"),
            Self::Vec3 => f.write_str("Vec3"),
            Self::Vec4 => f.write_str("Vec4"),
            Self::Quat => f.write_str("Quat"),
            Self::EntityId => f.write_str("EntityId"),
            Self::AssetRef => f.write_str("AssetRef"),
            Self::String => f.write_str("String"),
            Self::Named(name) => f.write_str(name),
            Self::List(inner) => write!(f, "List<{inner}>"),
            Self::Option(inner) => write!(f, "Option<{inner}>"),
            Self::Generic(GenericKind::Numeric) => f.write_str("Numeric"),
            Self::Generic(GenericKind::Scalar) => f.write_str("Scalar"),
        }
    }
}

impl Serialize for PinType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
