//! Type-directed conversion of RON syntax to serde's JSON form, recording positions by JSON Pointer.

use serde_json::{Map, Number, Value};

use concordia_reflect::{FieldInfo, Primitive, Shape, TypeInfo, VariantInfo, VariantShape};

use crate::syntax::{Body, Kind, Node, Pos};

/// A value did not have the shape its type expects.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ConvertError {
    pub(crate) pointer: String,
    pub(crate) pos: Pos,
    pub(crate) expected: String,
    pub(crate) found: String,
}

/// Converts nodes, remembering where each pointer came from.
#[derive(Default)]
pub(crate) struct Converter {
    spans: Vec<(String, Pos)>,
}

type Result<T> = std::result::Result<T, ConvertError>;

impl Converter {
    /// Position of `pointer`, or of its nearest recorded ancestor.
    pub(crate) fn pos_of(&self, pointer: &str) -> Pos {
        let mut best: Option<&(String, Pos)> = None;
        for span in &self.spans {
            let covers = pointer == span.0
                || pointer.starts_with(&format!("{}/", span.0))
                || span.0.is_empty();
            if covers && best.is_none_or(|b| span.0.len() > b.0.len()) {
                best = Some(span);
            }
        }
        best.map(|b| b.1).unwrap_or_default()
    }

    /// Converts `node` as a value of `info`.
    pub(crate) fn value(&mut self, node: &Node, info: &TypeInfo, at: &str) -> Result<Value> {
        self.spans.push((at.to_owned(), node.pos));
        self.shape(node, &info.shape, at)
    }

    fn shape(&mut self, node: &Node, shape: &Shape, at: &str) -> Result<Value> {
        match shape {
            Shape::Primitive(p) => primitive(node, *p, at),
            Shape::Struct { fields, .. } => self.structure(node, fields, at),
            Shape::TupleStruct { fields } | Shape::Tuple(fields) => {
                let infos: Vec<_> = fields.iter().map(|f| f.info()).collect();
                self.tuple(node, &infos, at)
            }
            Shape::Array { elem, len } => self.tuple(node, &vec![elem.info(); *len], at),
            Shape::Newtype { inner, .. } => match &node.kind {
                Kind::Call {
                    name: Some(_),
                    body: Body::Items(items),
                } if items.len() == 1 => self.value(&items[0], inner.info(), at),
                _ => self.shape(node, &inner.info().shape, at),
            },
            Shape::Unit => match &node.kind {
                Kind::Call {
                    body: Body::Items(items),
                    ..
                } if items.is_empty() => Ok(Value::Null),
                _ => Err(mismatch(node, "()", at)),
            },
            Shape::Enum { variants } => self.variant(node, variants, at),
            Shape::Option(inner) => match &node.kind {
                Kind::Ident(name) if name == "None" => Ok(Value::Null),
                Kind::Call {
                    name: Some(name),
                    body: Body::Items(items),
                } if name == "Some" && items.len() == 1 => self.value(&items[0], inner.info(), at),
                _ => self.value(node, inner.info(), at),
            },
            Shape::List(elem) => match &node.kind {
                Kind::List(items) => self.items(items, elem.info(), at),
                _ => Err(mismatch(node, "a list [...]", at)),
            },
            Shape::Map { value } => self.map(node, Some(value.info()), at),
            Shape::Any => self.any(node, at),
        }
    }

    fn structure(&mut self, node: &Node, fields: &[FieldInfo], at: &str) -> Result<Value> {
        let entries = match &node.kind {
            Kind::Call {
                body: Body::Fields(entries),
                ..
            } => entries.as_slice(),
            Kind::Call {
                body: Body::Items(items),
                ..
            } if items.is_empty() => &[],
            _ => return Err(mismatch(node, "a struct (field: value, ...)", at)),
        };
        let mut object = Map::new();
        for entry in entries {
            let here = child(at, &entry.name);
            if object.contains_key(&entry.name) {
                return Err(ConvertError {
                    pointer: here,
                    pos: entry.pos,
                    expected: "each field once".into(),
                    found: format!("field {:?} repeated", entry.name),
                });
            }
            let value = match fields.iter().find(|f| f.name == entry.name) {
                Some(field) => self.value(&entry.value, field.ty.info(), &here)?,
                None => {
                    self.spans.push((here.clone(), entry.pos));
                    self.any(&entry.value, &here)?
                }
            };
            object.insert(entry.name.clone(), value);
        }
        Ok(Value::Object(object))
    }

    fn tuple(&mut self, node: &Node, infos: &[&'static TypeInfo], at: &str) -> Result<Value> {
        let items = match &node.kind {
            Kind::Call {
                name: None,
                body: Body::Items(items),
            }
            | Kind::List(items)
                if items.len() == infos.len() =>
            {
                items
            }
            _ => return Err(mismatch(node, &format!("a tuple of {}", infos.len()), at)),
        };
        let values = items
            .iter()
            .zip(infos)
            .enumerate()
            .map(|(i, (item, info))| self.value(item, info, &child(at, &i.to_string())))
            .collect::<Result<Vec<_>>>()?;
        Ok(Value::Array(values))
    }

    fn items(&mut self, items: &[Node], elem: &TypeInfo, at: &str) -> Result<Value> {
        let values = items
            .iter()
            .enumerate()
            .map(|(i, item)| self.value(item, elem, &child(at, &i.to_string())))
            .collect::<Result<Vec<_>>>()?;
        Ok(Value::Array(values))
    }

    fn map(&mut self, node: &Node, value_info: Option<&TypeInfo>, at: &str) -> Result<Value> {
        let Kind::Map(entries) = &node.kind else {
            return Err(mismatch(node, "a map {\"key\": value, ...}", at));
        };
        let mut object = Map::new();
        for (key, value) in entries {
            let Kind::Str(key) = &key.kind else {
                return Err(mismatch(key, "a string key", at));
            };
            let here = child(at, key);
            let converted = match value_info {
                Some(info) => self.value(value, info, &here)?,
                None => self.any(value, &here)?,
            };
            object.insert(key.clone(), converted);
        }
        Ok(Value::Object(object))
    }

    fn variant(&mut self, node: &Node, variants: &[VariantInfo], at: &str) -> Result<Value> {
        let expected = || {
            let names: Vec<_> = variants.iter().map(|v| v.name).collect();
            format!("one of the variants {}", names.join(", "))
        };
        let (name, body) = match &node.kind {
            Kind::Ident(name) => (name, None),
            Kind::Call {
                name: Some(name),
                body,
            } => (name, Some(body)),
            _ => return Err(mismatch(node, &expected(), at)),
        };
        let variant = variants
            .iter()
            .find(|v| v.name == name)
            .ok_or_else(|| mismatch(node, &expected(), at))?;
        let here = child(at, name);
        let payload = match (&variant.shape, body) {
            (VariantShape::Unit, None) => return Ok(Value::String(name.clone())),
            (VariantShape::Newtype(ty), Some(Body::Items(items))) if items.len() == 1 => {
                self.value(&items[0], ty.info(), &here)?
            }
            (VariantShape::Tuple(tys), Some(body @ Body::Items(_))) => {
                let infos: Vec<_> = tys.iter().map(|t| t.info()).collect();
                let wrapped = Node {
                    kind: Kind::Call {
                        name: None,
                        body: body.clone(),
                    },
                    pos: node.pos,
                };
                self.tuple(&wrapped, &infos, &here)?
            }
            (VariantShape::Struct(fields), Some(body)) => {
                let wrapped = Node {
                    kind: Kind::Call {
                        name: None,
                        body: body.clone(),
                    },
                    pos: node.pos,
                };
                self.structure(&wrapped, fields, &here)?
            }
            _ => {
                return Err(mismatch(
                    node,
                    &format!("variant {name} in its declared form"),
                    at,
                ));
            }
        };
        Ok(Value::Object(Map::from_iter([(name.clone(), payload)])))
    }

    /// Converts without type guidance (for `Any` and unknown fields).
    fn any(&mut self, node: &Node, at: &str) -> Result<Value> {
        self.spans.push((at.to_owned(), node.pos));
        match &node.kind {
            Kind::Str(s) => Ok(Value::String(s.clone())),
            Kind::Num(text) => untyped_number(text).ok_or_else(|| mismatch(node, "a number", at)),
            Kind::Ident(i) if i == "true" || i == "false" => Ok(Value::Bool(i == "true")),
            Kind::Ident(i) if i == "None" => Ok(Value::Null),
            Kind::Ident(i) => Ok(Value::String(i.clone())),
            Kind::List(items)
            | Kind::Call {
                body: Body::Items(items),
                name: None,
            } => {
                let values = items
                    .iter()
                    .enumerate()
                    .map(|(i, n)| self.any(n, &child(at, &i.to_string())))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Value::Array(values))
            }
            Kind::Call {
                name: Some(name),
                body: Body::Items(items),
            } if name == "Some" && items.len() == 1 => self.any(&items[0], at),
            Kind::Call {
                name: Some(name),
                body,
            } => {
                let inner = Node {
                    kind: Kind::Call {
                        name: None,
                        body: body.clone(),
                    },
                    pos: node.pos,
                };
                Ok(Value::Object(Map::from_iter([(
                    name.clone(),
                    self.any(&inner, &child(at, name))?,
                )])))
            }
            Kind::Call {
                body: Body::Fields(fields),
                ..
            } => {
                let mut object = Map::new();
                for f in fields {
                    object.insert(f.name.clone(), self.any(&f.value, &child(at, &f.name))?);
                }
                Ok(Value::Object(object))
            }
            Kind::Map(_) => self.map(node, None, at),
        }
    }
}

fn primitive(node: &Node, p: Primitive, at: &str) -> Result<Value> {
    let expected = match p {
        Primitive::Bool => "a boolean",
        Primitive::F32 | Primitive::F64 => "a number",
        Primitive::String | Primitive::Char | Primitive::Uuid | Primitive::Pattern(_) => "a string",
        _ => "an integer",
    };
    let value = match (&node.kind, p) {
        (Kind::Ident(i), Primitive::Bool) if i == "true" || i == "false" => {
            Some(Value::Bool(i == "true"))
        }
        (
            Kind::Str(s),
            Primitive::String | Primitive::Char | Primitive::Uuid | Primitive::Pattern(_),
        ) => Some(Value::String(s.clone())),
        (Kind::Num(text), Primitive::F32) => text
            .parse::<f32>()
            .ok()
            .filter(|x| x.is_finite())
            .and_then(|x| Number::from_f64(f64::from(x)))
            .map(Value::Number),
        (Kind::Num(text), Primitive::F64) => text
            .parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number),
        (Kind::Num(text), _) if p.int_bounds().is_some() => integer(text),
        _ => None,
    };
    value.ok_or_else(|| mismatch(node, expected, at))
}

fn integer(text: &str) -> Option<Value> {
    text.parse::<i64>()
        .map(Value::from)
        .ok()
        .or_else(|| text.parse::<u64>().map(Value::from).ok())
}

fn untyped_number(text: &str) -> Option<Value> {
    integer(text).or_else(|| {
        text.parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number)
    })
}

fn mismatch(node: &Node, expected: &str, at: &str) -> ConvertError {
    ConvertError {
        pointer: at.to_owned(),
        pos: node.pos,
        expected: expected.to_owned(),
        found: found(node),
    }
}

/// Describes what a node is, for error messages.
pub(crate) fn found(node: &Node) -> String {
    match &node.kind {
        Kind::Str(_) => "a string".into(),
        Kind::Num(_) => "a number".into(),
        Kind::Ident(i) => format!("`{i}`"),
        Kind::Call { name: Some(n), .. } => format!("`{n}(...)`"),
        Kind::Call {
            body: Body::Fields(_),
            ..
        } => "a struct".into(),
        Kind::Call {
            body: Body::Items(items),
            ..
        } => format!("a tuple of {}", items.len()),
        Kind::List(_) => "a list".into(),
        Kind::Map(_) => "a map".into(),
    }
}

/// Appends an escaped RFC 6901 token.
pub(crate) fn child(at: &str, token: &str) -> String {
    format!("{at}/{}", token.replace('~', "~0").replace('/', "~1"))
}
