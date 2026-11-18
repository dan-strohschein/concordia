//! Shape validation of JSON against `TypeInfo`, with JSON Pointer errors for agents.

use serde_json::{Map, Value};

use crate::error::ReflectError;
use crate::info::{
    FieldInfo, Primitive, Range, Shape, TypeInfo, TypeRef, VariantInfo, VariantShape,
};

/// Checks that `value` has the shape `info` describes and respects declared ranges.
///
/// Patterns are checked only as strings; the owning type's parser enforces them.
pub fn validate_json(value: &Value, info: &TypeInfo) -> Result<(), ReflectError> {
    check_shape(value, &info.shape, "")
}

fn check_shape(value: &Value, shape: &Shape, at: &str) -> Result<(), ReflectError> {
    match shape {
        Shape::Primitive(p) => check_primitive(value, *p, at),
        Shape::Struct { fields, .. } => check_fields(value, fields, at),
        Shape::TupleStruct { fields } | Shape::Tuple(fields) => check_tuple(value, fields, at),
        Shape::Newtype { inner, .. } => check_ref(value, inner, at),
        Shape::Unit => expect(value.is_null(), "null", value, at),
        Shape::Enum { variants } => check_enum(value, variants, at),
        Shape::Option(_) if value.is_null() => Ok(()),
        Shape::Option(inner) => check_ref(value, inner, at),
        Shape::List(elem) => check_list(value, elem, at),
        Shape::Map { value: elem } => check_map(value, elem, at),
        Shape::Array { elem, len } => check_array(value, elem, *len, at),
        Shape::Any => Ok(()),
    }
}

fn check_ref(value: &Value, ty: &TypeRef, at: &str) -> Result<(), ReflectError> {
    check_shape(value, &ty.info().shape, at)
}

fn check_primitive(value: &Value, p: Primitive, at: &str) -> Result<(), ReflectError> {
    match p {
        Primitive::Bool => expect(value.is_boolean(), "boolean", value, at),
        Primitive::F32 | Primitive::F64 => expect(value.is_number(), "number", value, at),
        Primitive::String | Primitive::Uuid | Primitive::Pattern(_) => {
            expect(value.is_string(), "string", value, at)
        }
        Primitive::Char => {
            let ok = value.as_str().is_some_and(|s| s.chars().count() == 1);
            expect(ok, "a one-character string", value, at)
        }
        _ => check_integer(value, p, at),
    }
}

fn check_integer(value: &Value, p: Primitive, at: &str) -> Result<(), ReflectError> {
    let Some((min, max)) = p.int_bounds() else {
        return Ok(());
    };
    let as_int = value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from));
    match as_int {
        Some(n) if (min..=max).contains(&n) => Ok(()),
        Some(n) => Err(out_of_range(n as f64, min as f64, max as f64, at)),
        None => Err(invalid("integer", value, at)),
    }
}

fn check_fields(value: &Value, fields: &[FieldInfo], at: &str) -> Result<(), ReflectError> {
    let object = as_object(value, at)?;
    if let Some(extra) = object.keys().find(|k| !fields.iter().any(|f| f.name == *k)) {
        return Err(ReflectError::UnknownField {
            pointer: at.to_owned(),
            field: extra.clone(),
            expected: fields.iter().map(|f| f.name).collect(),
        });
    }
    fields.iter().try_for_each(|f| check_field(object, f, at))
}

fn check_field(
    object: &Map<String, Value>,
    field: &FieldInfo,
    at: &str,
) -> Result<(), ReflectError> {
    let Some(value) = object.get(field.name) else {
        return match field.optional {
            true => Ok(()),
            false => Err(ReflectError::MissingField {
                pointer: at.to_owned(),
                field: field.name.to_owned(),
            }),
        };
    };
    let here = child(at, field.name);
    check_ref(value, &field.ty, &here)?;
    match (field.range, value.as_f64()) {
        (Some(range), Some(n)) => check_range(n, range, &here),
        _ => Ok(()),
    }
}

fn check_range(n: f64, range: Range, at: &str) -> Result<(), ReflectError> {
    match (range.min..=range.max).contains(&n) {
        true => Ok(()),
        false => Err(out_of_range(n, range.min, range.max, at)),
    }
}

fn check_tuple(value: &Value, items: &[TypeRef], at: &str) -> Result<(), ReflectError> {
    let array = as_array_of_len(value, items.len(), at)?;
    array
        .iter()
        .zip(items)
        .enumerate()
        .try_for_each(|(i, (v, ty))| check_ref(v, ty, &child(at, &i.to_string())))
}

fn check_list(value: &Value, elem: &TypeRef, at: &str) -> Result<(), ReflectError> {
    let array = value
        .as_array()
        .ok_or_else(|| invalid("array", value, at))?;
    array
        .iter()
        .enumerate()
        .try_for_each(|(i, v)| check_ref(v, elem, &child(at, &i.to_string())))
}

fn check_array(value: &Value, elem: &TypeRef, len: usize, at: &str) -> Result<(), ReflectError> {
    let array = as_array_of_len(value, len, at)?;
    array
        .iter()
        .enumerate()
        .try_for_each(|(i, v)| check_ref(v, elem, &child(at, &i.to_string())))
}

fn check_map(value: &Value, elem: &TypeRef, at: &str) -> Result<(), ReflectError> {
    as_object(value, at)?
        .iter()
        .try_for_each(|(k, v)| check_ref(v, elem, &child(at, k)))
}

fn check_enum(value: &Value, variants: &[VariantInfo], at: &str) -> Result<(), ReflectError> {
    let names = || variant_list(variants);
    let (name, payload) = match value {
        Value::String(name) => (name.as_str(), None),
        Value::Object(map) if map.len() == 1 => {
            let (name, payload) = map.iter().next().expect("len checked");
            (name.as_str(), Some(payload))
        }
        _ => return Err(invalid(&names(), value, at)),
    };
    let variant = variants
        .iter()
        .find(|v| v.name == name)
        .ok_or_else(|| invalid(&names(), value, at))?;
    check_variant(variant, payload, &child(at, name), value, at)
}

fn check_variant(
    variant: &VariantInfo,
    payload: Option<&Value>,
    here: &str,
    whole: &Value,
    at: &str,
) -> Result<(), ReflectError> {
    match (&variant.shape, payload) {
        (VariantShape::Unit, None) => Ok(()),
        (VariantShape::Newtype(ty), Some(p)) => check_ref(p, ty, here),
        (VariantShape::Tuple(items), Some(p)) => check_tuple(p, items, here),
        (VariantShape::Struct(fields), Some(p)) => check_fields(p, fields, here),
        (VariantShape::Unit, Some(_)) => Err(invalid(&format!("\"{}\"", variant.name), whole, at)),
        (_, None) => Err(invalid(
            &format!("{{\"{}\": ...}}", variant.name),
            whole,
            at,
        )),
    }
}

fn variant_list(variants: &[VariantInfo]) -> String {
    let names: Vec<&str> = variants.iter().map(|v| v.name).collect();
    format!("one of the variants {}", names.join(", "))
}

fn as_object<'v>(value: &'v Value, at: &str) -> Result<&'v Map<String, Value>, ReflectError> {
    value
        .as_object()
        .ok_or_else(|| invalid("object", value, at))
}

fn as_array_of_len<'v>(
    value: &'v Value,
    len: usize,
    at: &str,
) -> Result<&'v Vec<Value>, ReflectError> {
    match value.as_array() {
        Some(array) if array.len() == len => Ok(array),
        Some(array) => Err(ReflectError::Invalid {
            pointer: at.to_owned(),
            expected: format!("array of {len}"),
            found: format!("array of {}", array.len()),
        }),
        None => Err(invalid(&format!("array of {len}"), value, at)),
    }
}

fn expect(ok: bool, expected: &str, value: &Value, at: &str) -> Result<(), ReflectError> {
    if ok {
        Ok(())
    } else {
        Err(invalid(expected, value, at))
    }
}

fn invalid(expected: &str, value: &Value, at: &str) -> ReflectError {
    ReflectError::Invalid {
        pointer: at.to_owned(),
        expected: expected.to_owned(),
        found: json_kind(value).to_owned(),
    }
}

fn out_of_range(value: f64, min: f64, max: f64, at: &str) -> ReflectError {
    ReflectError::OutOfRange {
        pointer: at.to_owned(),
        value,
        min,
        max,
    }
}

/// JSON type name of a value.
pub fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Appends an escaped RFC 6901 token to a pointer.
fn child(at: &str, token: &str) -> String {
    format!("{at}/{}", token.replace('~', "~0").replace('/', "~1"))
}
