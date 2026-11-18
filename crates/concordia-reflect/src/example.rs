//! Example values for any reflected type (used in agent-facing errors and docs).

use serde_json::{Map, Value, json};

use crate::info::{FieldInfo, Primitive, Shape, TypeInfo, VariantShape};

/// Recursion guard for self-referential types.
const MAX_DEPTH: usize = 16;

/// A value that validates against `info`'s schema; declared field defaults are used where present.
pub fn example_json(info: &TypeInfo) -> Value {
    example(&info.shape, 0)
}

fn example(shape: &Shape, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return Value::Null;
    }
    let next = depth + 1;
    match shape {
        Shape::Primitive(p) => primitive(*p),
        Shape::Struct { fields, .. } => fields_example(fields, next),
        Shape::TupleStruct { fields } | Shape::Tuple(fields) => Value::Array(
            fields
                .iter()
                .map(|f| example(&f.info().shape, next))
                .collect(),
        ),
        Shape::Newtype { inner, .. } => example(&inner.info().shape, next),
        Shape::Unit | Shape::Option(_) | Shape::Any => Value::Null,
        Shape::Enum { variants } => match variants.first().map(|v| (v.name, &v.shape)) {
            Some((name, VariantShape::Unit)) => json!(name),
            Some((name, VariantShape::Newtype(ty))) => {
                json!({ name: example(&ty.info().shape, next) })
            }
            Some((name, VariantShape::Tuple(items))) => {
                json!({ name: items.iter().map(|t| example(&t.info().shape, next)).collect::<Vec<_>>() })
            }
            Some((name, VariantShape::Struct(fields))) => {
                json!({ name: fields_example(fields, next) })
            }
            None => Value::Null,
        },
        Shape::List(_) => json!([]),
        Shape::Map { .. } => json!({}),
        Shape::Array { elem, len } => Value::Array(vec![example(&elem.info().shape, next); *len]),
    }
}

fn fields_example(fields: &[FieldInfo], depth: usize) -> Value {
    let object: Map<String, Value> = fields
        .iter()
        .map(|f| {
            let value = f
                .default
                .map_or_else(|| example(&f.ty.info().shape, depth), |default| default());
            (f.name.to_owned(), value)
        })
        .collect();
    Value::Object(object)
}

fn primitive(p: Primitive) -> Value {
    match p {
        Primitive::Bool => json!(false),
        Primitive::F32 | Primitive::F64 => json!(0.0),
        Primitive::String => json!(""),
        Primitive::Char => json!("a"),
        Primitive::Uuid => json!("01900000-0000-7000-8000-000000000000"),
        Primitive::Pattern(_) => json!(""),
        Primitive::NonZeroU32 | Primitive::NonZeroU64 => json!(1),
        _ => json!(0),
    }
}
