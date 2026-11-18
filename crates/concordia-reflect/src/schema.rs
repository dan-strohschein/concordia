//! JSON Schema (draft 2020-12) export, generated from `TypeInfo`.
//!
//! User types (structs and enums) become `$defs` entries; everything else is
//! inlined. Enums use serde's externally tagged form.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::info::{FieldInfo, Primitive, Shape, TypeInfo, TypeRef, VariantInfo, VariantShape};

/// The draft this crate emits.
pub const DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// A self-contained schema document for `info`.
pub fn schema_for(info: &'static TypeInfo) -> Value {
    let mut ctx = Ctx {
        root: info.name,
        defs: BTreeMap::new(),
    };
    let mut root = ctx.definition(info);
    root.insert("$schema".into(), DRAFT.into());
    let defs: Map<String, Value> = ctx
        .defs
        .into_iter()
        .filter_map(|(name, def)| def.map(|d| (name.to_owned(), d)))
        .collect();
    if !defs.is_empty() {
        root.insert("$defs".into(), Value::Object(defs));
    }
    Value::Object(root)
}

/// One row of the registry index.
pub fn index_entry(info: &TypeInfo) -> Value {
    json!({
        "name": info.name,
        "kind": kind(&info.shape),
        "description": info.docs.lines().next().unwrap_or(""),
    })
}

fn kind(shape: &Shape) -> &'static str {
    match shape {
        Shape::Primitive(_) => "primitive",
        Shape::Struct { .. } | Shape::TupleStruct { .. } | Shape::Newtype { .. } | Shape::Unit => {
            "struct"
        }
        Shape::Enum { .. } => "enum",
        Shape::Array { .. } => "array",
        Shape::Any => "any",
        Shape::Option(_) | Shape::List(_) | Shape::Map { .. } | Shape::Tuple(_) => "structural",
    }
}

struct Ctx {
    root: &'static str,
    /// `None` marks a definition in progress (breaks recursion).
    defs: BTreeMap<&'static str, Option<Value>>,
}

impl Ctx {
    fn definition(&mut self, info: &'static TypeInfo) -> Map<String, Value> {
        let mut schema = self.body(&info.shape);
        schema.insert("title".into(), info.name.into());
        describe(&mut schema, info.docs);
        schema.insert("x-concordia-type".into(), info.name.into());
        schema
    }

    fn reference(&mut self, ty: &TypeRef) -> Map<String, Value> {
        let info = ty.info();
        if !info.is_definition() {
            let mut schema = self.body(&info.shape);
            describe(&mut schema, info.docs);
            return schema;
        }
        if info.name == self.root {
            return object([("$ref", json!("#"))]);
        }
        if !self.defs.contains_key(info.name) {
            self.defs.insert(info.name, None);
            let def = self.definition(info);
            self.defs.insert(info.name, Some(Value::Object(def)));
        }
        let pointer = info.name.replace('~', "~0").replace('/', "~1");
        object([("$ref", json!(format!("#/$defs/{pointer}")))])
    }

    fn body(&mut self, shape: &Shape) -> Map<String, Value> {
        match shape {
            Shape::Primitive(p) => primitive(*p),
            Shape::Struct { fields, .. } => self.fields(fields),
            Shape::TupleStruct { fields } | Shape::Tuple(fields) => self.tuple(fields),
            Shape::Newtype { inner, .. } => self.reference(inner),
            Shape::Unit => object([("type", json!("null"))]),
            Shape::Enum { variants } => self.variants(variants),
            Shape::Option(inner) => {
                let inner = Value::Object(self.reference(inner));
                object([("anyOf", json!([{ "type": "null" }, inner]))])
            }
            Shape::List(elem) => {
                let items = Value::Object(self.reference(elem));
                object([("type", json!("array")), ("items", items)])
            }
            Shape::Map { value } => {
                let values = Value::Object(self.reference(value));
                object([("type", json!("object")), ("additionalProperties", values)])
            }
            Shape::Array { elem, len } => {
                let items = Value::Object(self.reference(elem));
                object([
                    ("type", json!("array")),
                    ("items", items),
                    ("minItems", json!(len)),
                    ("maxItems", json!(len)),
                ])
            }
            Shape::Any => Map::new(),
        }
    }

    fn fields(&mut self, fields: &[FieldInfo]) -> Map<String, Value> {
        let properties: Map<String, Value> = fields
            .iter()
            .map(|f| (f.name.to_owned(), Value::Object(self.field(f))))
            .collect();
        let required: Vec<&str> = fields
            .iter()
            .filter(|f| !f.optional)
            .map(|f| f.name)
            .collect();
        let mut schema = object([
            ("type", json!("object")),
            ("properties", Value::Object(properties)),
        ]);
        if !required.is_empty() {
            schema.insert("required".into(), json!(required));
        }
        schema.insert("additionalProperties".into(), json!(false));
        schema
    }

    fn field(&mut self, field: &FieldInfo) -> Map<String, Value> {
        let mut schema = self.reference(&field.ty);
        let type_docs = schema
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned);
        match type_docs {
            Some(type_docs) if !field.docs.is_empty() => {
                describe(&mut schema, &format!("{}\n\n{type_docs}", field.docs));
            }
            _ => describe(&mut schema, field.docs),
        }
        if let Some(default) = field.default {
            schema.insert("default".into(), default());
        }
        if let Some(range) = field.range {
            schema.insert("minimum".into(), json!(range.min));
            schema.insert("maximum".into(), json!(range.max));
        }
        if let Some(widget) = field.widget.or(field.ty.info().widget) {
            schema.insert("x-concordia-widget".into(), json!(widget));
        }
        schema
    }

    fn tuple(&mut self, items: &[TypeRef]) -> Map<String, Value> {
        let prefix: Vec<Value> = items
            .iter()
            .map(|t| Value::Object(self.reference(t)))
            .collect();
        object([
            ("type", json!("array")),
            ("prefixItems", json!(prefix)),
            ("items", json!(false)),
            ("minItems", json!(items.len())),
            ("maxItems", json!(items.len())),
        ])
    }

    fn variants(&mut self, variants: &[VariantInfo]) -> Map<String, Value> {
        let one_of: Vec<Value> = variants
            .iter()
            .map(|v| Value::Object(self.variant(v)))
            .collect();
        object([("oneOf", json!(one_of))])
    }

    fn variant(&mut self, variant: &VariantInfo) -> Map<String, Value> {
        let payload = match &variant.shape {
            VariantShape::Unit => {
                let mut schema = object([("const", json!(variant.name))]);
                describe(&mut schema, variant.docs);
                return schema;
            }
            VariantShape::Newtype(ty) => self.reference(ty),
            VariantShape::Tuple(items) => self.tuple(items),
            VariantShape::Struct(fields) => self.fields(fields),
        };
        let mut schema = object([
            ("type", json!("object")),
            ("properties", json!({ variant.name: payload })),
            ("required", json!([variant.name])),
            ("additionalProperties", json!(false)),
        ]);
        describe(&mut schema, variant.docs);
        schema
    }
}

fn primitive(p: Primitive) -> Map<String, Value> {
    if let Some((min, max)) = p.int_bounds() {
        return object([
            ("type", json!("integer")),
            ("minimum", int(min)),
            ("maximum", int(max)),
        ]);
    }
    match p {
        Primitive::Bool => object([("type", json!("boolean"))]),
        Primitive::F32 | Primitive::F64 => object([("type", json!("number"))]),
        Primitive::Char => object([
            ("type", json!("string")),
            ("minLength", json!(1)),
            ("maxLength", json!(1)),
        ]),
        Primitive::Uuid => object([("type", json!("string")), ("format", json!("uuid"))]),
        Primitive::Pattern(pattern) => {
            object([("type", json!("string")), ("pattern", json!(pattern))])
        }
        _ => object([("type", json!("string"))]),
    }
}

/// Exact JSON integer for any bound of a 64-bit-or-smaller integer type.
fn int(n: i128) -> Value {
    i64::try_from(n)
        .map(Value::from)
        .or_else(|_| u64::try_from(n).map(Value::from))
        .unwrap_or(Value::Null)
}

fn describe(schema: &mut Map<String, Value>, docs: &str) {
    if !docs.is_empty() {
        schema.insert("description".into(), docs.into());
    }
}

fn object<const N: usize>(entries: [(&str, Value); N]) -> Map<String, Value> {
    entries
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}
