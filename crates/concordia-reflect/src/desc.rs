//! Serializable type descriptions (ADR-0062): how script modules declare their types, and how a
//! registered type can be sent to one. A [`TypeDesc`] mirrors a [`TypeInfo`] without pointers.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::info::{
    DefaultValue, FieldInfo, Primitive, Range, Shape, TypeInfo, TypeRef, VariantInfo, VariantShape,
};
use crate::registry::{RegistryError, TypeRegistry};

/// A named type's description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeDesc {
    /// Stable name (unique in a registry).
    pub name: String,
    /// Doc comment text.
    #[serde(default)]
    pub docs: String,
    /// Wire shape.
    pub shape: ShapeDesc,
    /// Editor widget hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<String>,
    /// Discovery category.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// The whole value's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

/// A shape, as in [`crate::Shape`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShapeDesc {
    /// A scalar.
    Primitive(PrimitiveDesc),
    /// Named fields.
    Struct {
        /// Fields in declaration order.
        fields: Vec<FieldDesc>,
        /// Unknown fields are rejected.
        #[serde(default)]
        deny_unknown: bool,
    },
    /// Positional fields (2+).
    TupleStruct(Vec<TypeRefDesc>),
    /// A one-field wrapper.
    Newtype {
        /// The wrapped type.
        inner: TypeRefDesc,
        /// `#[serde(transparent)]`.
        #[serde(default)]
        transparent: bool,
    },
    /// A unit struct (a tag when used as a component).
    Unit,
    /// An externally tagged enum.
    Enum(Vec<VariantDesc>),
    /// `Option<T>`.
    Option(Box<TypeRefDesc>),
    /// `Vec<T>`.
    List(Box<TypeRefDesc>),
    /// A string-keyed map.
    Map(Box<TypeRefDesc>),
    /// A fixed-length array.
    Array {
        /// Element type.
        elem: Box<TypeRefDesc>,
        /// Exact length.
        len: u32,
    },
    /// A tuple.
    Tuple(Vec<TypeRefDesc>),
    /// Any JSON value.
    Any,
}

/// Scalar kinds, as in [`crate::Primitive`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub enum PrimitiveDesc {
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    NonZeroU32,
    NonZeroU64,
    F32,
    F64,
    String,
    Char,
    Uuid,
    /// A string matching a regular expression.
    Pattern(String),
}

/// A reference to another type: a registered named type, or an inline structural one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TypeRefDesc {
    /// A named type already in the registry (e.g. `f32`, `Vec3`, `Transform`).
    Named(String),
    /// A structural type (`Option`, list, map, array, tuple) described inline.
    Structural(Box<ShapeDesc>),
}

/// A named field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDesc {
    /// Field name.
    pub name: String,
    /// Field type.
    pub ty: TypeRefDesc,
    /// Doc comment text.
    #[serde(default)]
    pub docs: String,
    /// Default value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// Inclusive numeric bounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<RangeDesc>,
    /// Editor widget hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<String>,
    /// May be absent on input.
    #[serde(default)]
    pub optional: bool,
}

/// Inclusive numeric bounds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RangeDesc {
    /// Lower bound.
    pub min: f64,
    /// Upper bound.
    pub max: f64,
}

/// An enum variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantDesc {
    /// Variant name.
    pub name: String,
    /// Doc comment text.
    #[serde(default)]
    pub docs: String,
    /// Payload.
    pub shape: VariantShapeDesc,
}

/// An enum variant's payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VariantShapeDesc {
    /// No payload.
    Unit,
    /// One value.
    Newtype(TypeRefDesc),
    /// Positional values.
    Tuple(Vec<TypeRefDesc>),
    /// Named fields.
    Struct(Vec<FieldDesc>),
}

impl TypeInfo {
    /// This type's description; named types it refers to appear by name.
    pub fn to_desc(&self) -> TypeDesc {
        TypeDesc {
            name: self.name.to_owned(),
            docs: self.docs.to_owned(),
            shape: shape_desc(&self.shape),
            widget: self.widget.map(str::to_owned),
            category: self.category.map(str::to_owned),
            default: self.default.map(|d| d.get()),
        }
    }
}

fn shape_desc(shape: &Shape) -> ShapeDesc {
    match shape {
        Shape::Primitive(p) => ShapeDesc::Primitive(primitive_desc(*p)),
        Shape::Struct {
            fields,
            deny_unknown,
        } => ShapeDesc::Struct {
            fields: fields.iter().map(field_desc).collect(),
            deny_unknown: *deny_unknown,
        },
        Shape::TupleStruct { fields } => ShapeDesc::TupleStruct(refs_desc(fields)),
        Shape::Newtype { inner, transparent } => ShapeDesc::Newtype {
            inner: ref_desc(inner),
            transparent: *transparent,
        },
        Shape::Unit => ShapeDesc::Unit,
        Shape::Enum { variants } => ShapeDesc::Enum(variants.iter().map(variant_desc).collect()),
        Shape::Option(r) => ShapeDesc::Option(Box::new(ref_desc(r))),
        Shape::List(r) => ShapeDesc::List(Box::new(ref_desc(r))),
        Shape::Map { value } => ShapeDesc::Map(Box::new(ref_desc(value))),
        Shape::Array { elem, len } => ShapeDesc::Array {
            elem: Box::new(ref_desc(elem)),
            len: u32::try_from(*len).unwrap_or(u32::MAX),
        },
        Shape::Tuple(refs) => ShapeDesc::Tuple(refs_desc(refs)),
        Shape::Any => ShapeDesc::Any,
    }
}

fn primitive_desc(p: Primitive) -> PrimitiveDesc {
    match p {
        Primitive::Bool => PrimitiveDesc::Bool,
        Primitive::I8 => PrimitiveDesc::I8,
        Primitive::I16 => PrimitiveDesc::I16,
        Primitive::I32 => PrimitiveDesc::I32,
        Primitive::I64 => PrimitiveDesc::I64,
        Primitive::U8 => PrimitiveDesc::U8,
        Primitive::U16 => PrimitiveDesc::U16,
        Primitive::U32 => PrimitiveDesc::U32,
        Primitive::U64 => PrimitiveDesc::U64,
        Primitive::NonZeroU32 => PrimitiveDesc::NonZeroU32,
        Primitive::NonZeroU64 => PrimitiveDesc::NonZeroU64,
        Primitive::F32 => PrimitiveDesc::F32,
        Primitive::F64 => PrimitiveDesc::F64,
        Primitive::String => PrimitiveDesc::String,
        Primitive::Char => PrimitiveDesc::Char,
        Primitive::Uuid => PrimitiveDesc::Uuid,
        Primitive::Pattern(re) => PrimitiveDesc::Pattern(re.to_owned()),
    }
}

fn ref_desc(r: &TypeRef) -> TypeRefDesc {
    let info = r.info();
    if info.structural {
        TypeRefDesc::Structural(Box::new(shape_desc(&info.shape)))
    } else {
        TypeRefDesc::Named(info.name.to_owned())
    }
}

fn refs_desc(refs: &[TypeRef]) -> Vec<TypeRefDesc> {
    refs.iter().map(ref_desc).collect()
}

fn field_desc(f: &FieldInfo) -> FieldDesc {
    FieldDesc {
        name: f.name.to_owned(),
        ty: ref_desc(&f.ty),
        docs: f.docs.to_owned(),
        default: f.default.map(|d| d.get()),
        range: f.range.map(|r| RangeDesc {
            min: r.min,
            max: r.max,
        }),
        widget: f.widget.map(str::to_owned),
        optional: f.optional,
    }
}

fn variant_desc(v: &VariantInfo) -> VariantDesc {
    VariantDesc {
        name: v.name.to_owned(),
        docs: v.docs.to_owned(),
        shape: match &v.shape {
            VariantShape::Unit => VariantShapeDesc::Unit,
            VariantShape::Newtype(r) => VariantShapeDesc::Newtype(ref_desc(r)),
            VariantShape::Tuple(refs) => VariantShapeDesc::Tuple(refs_desc(refs)),
            VariantShape::Struct(fields) => {
                VariantShapeDesc::Struct(fields.iter().map(field_desc).collect())
            }
        },
    }
}

/// Rust path recorded for runtime-described types.
pub(crate) const DYNAMIC_PATH: &str = "<dynamic>";

impl TypeDesc {
    /// Builds a `TypeInfo` (leaked: types live as long as the process), resolving named
    /// references in `registry`. Nothing is registered.
    pub fn to_info(&self, registry: &TypeRegistry) -> Result<&'static TypeInfo, RegistryError> {
        let builder = Builder {
            registry,
            owner: &self.name,
        };
        builder.check_name(&self.name, "type")?;
        let shape = builder.shape(&self.shape)?;
        let mut info = TypeInfo::named(leak(&self.name), DYNAMIC_PATH, leak(&self.docs), shape);
        info.widget = self.widget.as_deref().map(leak);
        info.category = self.category.as_deref().map(leak);
        if let Some(default) = &self.default {
            builder.check_default(default, &info, &self.name)?;
            info.default = Some(DefaultValue::Json(Box::leak(Box::new(default.clone()))));
        }
        Ok(Box::leak(Box::new(info)))
    }
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

/// Turns descriptions into leaked infos for one owning type.
struct Builder<'a> {
    registry: &'a TypeRegistry,
    owner: &'a str,
}

impl Builder<'_> {
    fn invalid(&self, message: String) -> RegistryError {
        RegistryError::InvalidDesc {
            name: self.owner.to_owned(),
            message,
        }
    }

    fn check_name(&self, name: &str, what: &str) -> Result<(), RegistryError> {
        if name.trim().is_empty() || name.trim() != name {
            return Err(self.invalid(format!("{what} name {name:?} is empty or padded")));
        }
        Ok(())
    }

    fn check_unique<'n>(
        &self,
        names: impl Iterator<Item = &'n str>,
        what: &str,
    ) -> Result<(), RegistryError> {
        let mut seen = std::collections::BTreeSet::new();
        for name in names {
            self.check_name(name, what)?;
            if !seen.insert(name) {
                return Err(self.invalid(format!("duplicate {what} {name:?}")));
            }
        }
        Ok(())
    }

    fn check_default(
        &self,
        default: &Value,
        info: &TypeInfo,
        at: &str,
    ) -> Result<(), RegistryError> {
        crate::validate::validate_json(default, info)
            .map_err(|e| self.invalid(format!("default of {at} is invalid: {e}")))
    }

    fn shape(&self, shape: &ShapeDesc) -> Result<Shape, RegistryError> {
        Ok(match shape {
            ShapeDesc::Primitive(p) => Shape::Primitive(primitive(p)),
            ShapeDesc::Struct {
                fields,
                deny_unknown,
            } => Shape::Struct {
                fields: self.fields(fields)?,
                deny_unknown: *deny_unknown,
            },
            ShapeDesc::TupleStruct(refs) if refs.len() < 2 => {
                return Err(self.invalid("a tuple struct needs at least two fields".into()));
            }
            ShapeDesc::TupleStruct(refs) => Shape::TupleStruct {
                fields: self.refs(refs)?,
            },
            ShapeDesc::Newtype { inner, transparent } => Shape::Newtype {
                inner: self.reference(inner)?,
                transparent: *transparent,
            },
            ShapeDesc::Unit => Shape::Unit,
            ShapeDesc::Enum(variants) if variants.is_empty() => {
                return Err(self.invalid("an enum needs at least one variant".into()));
            }
            ShapeDesc::Enum(variants) => Shape::Enum {
                variants: self.variants(variants)?,
            },
            ShapeDesc::Option(r) => Shape::Option(self.reference(r)?),
            ShapeDesc::List(r) => Shape::List(self.reference(r)?),
            ShapeDesc::Map(r) => Shape::Map {
                value: self.reference(r)?,
            },
            ShapeDesc::Array { elem, len } => Shape::Array {
                elem: self.reference(elem)?,
                len: *len as usize,
            },
            ShapeDesc::Tuple(refs) => Shape::Tuple(self.refs(refs)?),
            ShapeDesc::Any => Shape::Any,
        })
    }

    fn reference(&self, r: &TypeRefDesc) -> Result<TypeRef, RegistryError> {
        match r {
            TypeRefDesc::Named(name) => self
                .registry
                .get_by_name(name)
                .map(|reg| TypeRef::to(reg.info()))
                .or_else(|| builtin(name))
                .ok_or_else(|| RegistryError::UnknownType {
                    name: name.clone(),
                    referenced_by: self.owner.to_owned(),
                }),
            TypeRefDesc::Structural(shape) => {
                let shape = self.shape(shape)?;
                let name = structural_name(&shape);
                Ok(TypeRef::to(Box::leak(Box::new(TypeInfo::structural(
                    leak(&name),
                    DYNAMIC_PATH,
                    shape,
                )))))
            }
        }
    }

    fn refs(&self, refs: &[TypeRefDesc]) -> Result<Vec<TypeRef>, RegistryError> {
        refs.iter().map(|r| self.reference(r)).collect()
    }

    fn fields(&self, fields: &[FieldDesc]) -> Result<Vec<FieldInfo>, RegistryError> {
        self.check_unique(fields.iter().map(|f| f.name.as_str()), "field")?;
        fields.iter().map(|f| self.field(f)).collect()
    }

    fn field(&self, f: &FieldDesc) -> Result<FieldInfo, RegistryError> {
        let ty = self.reference(&f.ty)?;
        let range = match f.range {
            Some(r) if r.min.partial_cmp(&r.max).is_none_or(|o| o.is_gt()) => {
                return Err(self.invalid(format!("field {:?} has an empty range", f.name)));
            }
            r => r.map(|r| Range {
                min: r.min,
                max: r.max,
            }),
        };
        let default = match &f.default {
            Some(value) => {
                self.check_default(value, ty.info(), &f.name)?;
                Some(DefaultValue::Json(Box::leak(Box::new(value.clone()))))
            }
            None => None,
        };
        Ok(FieldInfo {
            name: leak(&f.name),
            ty,
            docs: leak(&f.docs),
            default,
            range,
            widget: f.widget.as_deref().map(leak),
            optional: f.optional,
        })
    }

    fn variants(&self, variants: &[VariantDesc]) -> Result<Vec<VariantInfo>, RegistryError> {
        self.check_unique(variants.iter().map(|v| v.name.as_str()), "variant")?;
        variants
            .iter()
            .map(|v| {
                Ok(VariantInfo {
                    name: leak(&v.name),
                    docs: leak(&v.docs),
                    shape: match &v.shape {
                        VariantShapeDesc::Unit => VariantShape::Unit,
                        VariantShapeDesc::Newtype(r) => VariantShape::Newtype(self.reference(r)?),
                        VariantShapeDesc::Tuple(refs) => VariantShape::Tuple(self.refs(refs)?),
                        VariantShapeDesc::Struct(fields) => {
                            VariantShape::Struct(self.fields(fields)?)
                        }
                    },
                })
            })
            .collect()
    }
}

fn primitive(p: &PrimitiveDesc) -> Primitive {
    match p {
        PrimitiveDesc::Bool => Primitive::Bool,
        PrimitiveDesc::I8 => Primitive::I8,
        PrimitiveDesc::I16 => Primitive::I16,
        PrimitiveDesc::I32 => Primitive::I32,
        PrimitiveDesc::I64 => Primitive::I64,
        PrimitiveDesc::U8 => Primitive::U8,
        PrimitiveDesc::U16 => Primitive::U16,
        PrimitiveDesc::U32 => Primitive::U32,
        PrimitiveDesc::U64 => Primitive::U64,
        PrimitiveDesc::NonZeroU32 => Primitive::NonZeroU32,
        PrimitiveDesc::NonZeroU64 => Primitive::NonZeroU64,
        PrimitiveDesc::F32 => Primitive::F32,
        PrimitiveDesc::F64 => Primitive::F64,
        PrimitiveDesc::String => Primitive::String,
        PrimitiveDesc::Char => Primitive::Char,
        PrimitiveDesc::Uuid => Primitive::Uuid,
        PrimitiveDesc::Pattern(re) => Primitive::Pattern(leak(re)),
    }
}

/// The name a Rust structural type of this shape has (`Option<f32>`, `Vec<T>`, ...).
fn structural_name(shape: &Shape) -> String {
    match shape {
        Shape::Option(r) => format!("Option<{}>", r.name()),
        Shape::List(r) => format!("Vec<{}>", r.name()),
        Shape::Map { value } => format!("Map<{}>", value.name()),
        Shape::Array { elem, len } => format!("[{}; {len}]", elem.name()),
        Shape::Tuple(refs) => {
            let names: Vec<&str> = refs.iter().map(TypeRef::name).collect();
            format!("({})", names.join(", "))
        }
        _ => "<inline>".to_owned(),
    }
}

/// Built-in types every description may name, registered or not: scalars, `Any` and math types.
fn builtin(name: &str) -> Option<TypeRef> {
    use concordia_core::math::{Mat4, Quat, Vec2, Vec3, Vec4};
    Some(match name {
        "bool" => TypeRef::of::<bool>(),
        "i8" => TypeRef::of::<i8>(),
        "i16" => TypeRef::of::<i16>(),
        "i32" => TypeRef::of::<i32>(),
        "i64" => TypeRef::of::<i64>(),
        "u8" => TypeRef::of::<u8>(),
        "u16" => TypeRef::of::<u16>(),
        "u32" => TypeRef::of::<u32>(),
        "u64" => TypeRef::of::<u64>(),
        "NonZeroU32" => TypeRef::of::<std::num::NonZeroU32>(),
        "NonZeroU64" => TypeRef::of::<std::num::NonZeroU64>(),
        "f32" => TypeRef::of::<f32>(),
        "f64" => TypeRef::of::<f64>(),
        "char" => TypeRef::of::<char>(),
        "String" => TypeRef::of::<String>(),
        "Any" => TypeRef::of::<Value>(),
        "EntityId" => TypeRef::of::<concordia_core::id::EntityId>(),
        "Vec2" => TypeRef::of::<Vec2>(),
        "Vec3" => TypeRef::of::<Vec3>(),
        "Vec4" => TypeRef::of::<Vec4>(),
        "Quat" => TypeRef::of::<Quat>(),
        "Mat4" => TypeRef::of::<Mat4>(),
        _ => return None,
    })
}
