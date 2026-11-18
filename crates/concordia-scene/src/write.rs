//! The canonical writer: 4-space indent, sorted keys, trailing commas, shortest round-trip floats.

use concordia_core::id::EntityId;
use concordia_reflect::TypeRegistry;
use concordia_reflect::{
    DynReflect, FieldInfo, Primitive, Shape, TypeInfo, VariantShape, find_non_finite,
};
use serde_json::Value;

use crate::error::SceneError;
use crate::model::{EntityDoc, FORMAT_VERSION, PrefabDoc, SceneDoc};
use crate::read::{read_prefab, read_scene};

const INDENT: &str = "    ";

/// Writes a scene in canonical form.
pub fn write_scene(doc: &SceneDoc) -> Result<String, SceneError> {
    let mut out = header("scene");
    out.push_str("Scene(\n");
    out.push_str(&format!(
        "    format: {FORMAT_VERSION},\n    id: {},\n    name: {},\n",
        quote(&doc.id.to_string()),
        quote(&doc.name)
    ));
    write_entities(&mut out, &doc.entities)?;
    out.push_str(")\n");
    Ok(out)
}

/// Writes a prefab in canonical form.
pub fn write_prefab(doc: &PrefabDoc) -> Result<String, SceneError> {
    let mut out = header("prefab");
    out.push_str("Prefab(\n");
    out.push_str(&format!(
        "    format: {FORMAT_VERSION},\n    id: {},\n    name: {},\n    root: {},\n",
        quote(&doc.id.to_string()),
        quote(&doc.name),
        quote(&doc.root.to_string())
    ));
    write_entities(&mut out, &doc.entities)?;
    out.push_str(")\n");
    Ok(out)
}

/// Rewrites scene or prefab text in canonical form.
pub fn format(text: &str, registry: &TypeRegistry) -> Result<String, SceneError> {
    match read_scene(text, registry) {
        Ok(doc) => write_scene(&doc),
        Err(scene_err) => match read_prefab(text, registry) {
            Ok(doc) => write_prefab(&doc),
            Err(_) => Err(scene_err),
        },
    }
}

fn header(kind: &str) -> String {
    format!(
        "// Concordia {kind}, format {FORMAT_VERSION}. Canonical form: `concordia fmt` rewrites this file.\n"
    )
}

fn write_entities(out: &mut String, entities: &[EntityDoc]) -> Result<(), SceneError> {
    if entities.is_empty() {
        out.push_str("    entities: [],\n");
        return Ok(());
    }
    let mut sorted: Vec<&EntityDoc> = entities.iter().collect();
    sorted.sort_by_key(|e| e.id);
    out.push_str("    entities: [\n");
    for entity in sorted {
        out.push_str(&format!(
            "        Entity(\n            id: {},\n",
            quote(&entity.id.to_string())
        ));
        write_components(out, entity)?;
        out.push_str("        ),\n");
    }
    out.push_str("    ],\n");
    Ok(())
}

fn write_components(out: &mut String, entity: &EntityDoc) -> Result<(), SceneError> {
    if entity.components.is_empty() {
        out.push_str("            components: {},\n");
        return Ok(());
    }
    out.push_str("            components: {\n");
    for (name, value) in &entity.components {
        let text = component(entity.id, name, value.as_ref())?;
        out.push_str(&format!("                {}: {text},\n", quote(name)));
    }
    out.push_str("            },\n");
    Ok(())
}

fn component(entity: EntityId, name: &str, value: &dyn DynReflect) -> Result<String, SceneError> {
    if let Some(pointer) = find_non_finite(value) {
        return Err(SceneError::NonFinite {
            entity,
            component: name.to_owned(),
            pointer,
        });
    }
    Ok(Writer { indent: 4 }.value(&value.to_json()?, value.info()))
}

/// Renders a document value as canonical RON, naming the root struct after its type.
pub(crate) fn document(json: &Value, info: &TypeInfo) -> String {
    let writer = Writer { indent: 0 };
    let text = match &info.shape {
        Shape::Struct { fields, .. } => writer.structure(Some(info.name), json, fields),
        _ => writer.value(json, info),
    };
    format!("{text}\n")
}

/// Renders one value; the first line is unindented, closing brackets sit at `indent`.
#[derive(Clone, Copy)]
struct Writer {
    indent: usize,
}

impl Writer {
    fn inner(self) -> Self {
        Self {
            indent: self.indent + 1,
        }
    }

    fn pad(self) -> String {
        INDENT.repeat(self.indent)
    }

    fn value(self, json: &Value, info: &TypeInfo) -> String {
        match &info.shape {
            Shape::Primitive(p) => primitive(json, *p),
            Shape::Struct { fields, .. } => self.structure(None, json, fields),
            Shape::TupleStruct { fields } | Shape::Tuple(fields) => {
                let infos: Vec<_> = fields.iter().map(|f| f.info()).collect();
                self.tuple(None, json, &infos)
            }
            Shape::Array { elem, len } => self.tuple(None, json, &vec![elem.info(); *len]),
            Shape::Newtype { inner, .. } => self.value(json, inner.info()),
            Shape::Unit => "()".into(),
            Shape::Enum { variants } => self.variant(json, variants),
            Shape::Option(inner) => match json {
                Value::Null => "None".into(),
                other => format!("Some({})", self.value(other, inner.info())),
            },
            Shape::List(elem) => self.list(json, Some(elem.info())),
            Shape::Map { value } => self.map(json, Some(value.info())),
            Shape::Any => self.any(json),
        }
    }

    fn structure(self, name: Option<&str>, json: &Value, fields: &[FieldInfo]) -> String {
        let prefix = name.unwrap_or("");
        let Some(object) = json.as_object().filter(|o| !o.is_empty()) else {
            return format!("{prefix}()");
        };
        let inner = self.inner();
        let mut out = format!("{prefix}(\n");
        for (key, value) in object {
            let rendered = match fields.iter().find(|f| f.name == key) {
                Some(field) => inner.value(value, field.ty.info()),
                None => inner.any(value),
            };
            out.push_str(&format!("{}{key}: {rendered},\n", inner.pad()));
        }
        out.push_str(&format!("{})", self.pad()));
        out
    }

    fn tuple(self, name: Option<&str>, json: &Value, infos: &[&'static TypeInfo]) -> String {
        let items = json.as_array().map(Vec::as_slice).unwrap_or_default();
        let rendered: Vec<String> = items
            .iter()
            .zip(infos)
            .map(|(v, i)| self.inner().value(v, i))
            .collect();
        self.positional(name.unwrap_or(""), &rendered)
    }

    /// `(a, b)` when every item fits on one line, otherwise one item per line.
    fn positional(self, prefix: &str, rendered: &[String]) -> String {
        if rendered.iter().all(|r| !r.contains('\n')) {
            return format!("{prefix}({})", rendered.join(", "));
        }
        let inner = self.inner();
        let body: String = rendered
            .iter()
            .map(|r| format!("{}{r},\n", inner.pad()))
            .collect();
        format!("{prefix}(\n{body}{})", self.pad())
    }

    fn variant(self, json: &Value, variants: &[concordia_reflect::VariantInfo]) -> String {
        let (name, payload) = match json {
            Value::String(name) => return name.clone(),
            Value::Object(map) if map.len() == 1 => map
                .iter()
                .next()
                .map(|(k, v)| (k.as_str(), v))
                .unwrap_or(("", json)),
            other => return self.any(other),
        };
        match variants.iter().find(|v| v.name == name).map(|v| &v.shape) {
            Some(VariantShape::Newtype(ty)) => {
                format!("{name}({})", self.value(payload, ty.info()))
            }
            Some(VariantShape::Tuple(tys)) => {
                let infos: Vec<_> = tys.iter().map(|t| t.info()).collect();
                self.tuple(Some(name), payload, &infos)
            }
            Some(VariantShape::Struct(fields)) => self.structure(Some(name), payload, fields),
            Some(VariantShape::Unit) | None => name.to_owned(),
        }
    }

    fn list(self, json: &Value, elem: Option<&TypeInfo>) -> String {
        let items = json.as_array().map(Vec::as_slice).unwrap_or_default();
        if items.is_empty() {
            return "[]".into();
        }
        let inner = self.inner();
        let mut out = "[\n".to_owned();
        for item in items {
            let rendered = elem.map_or_else(|| inner.any(item), |info| inner.value(item, info));
            out.push_str(&format!("{}{rendered},\n", inner.pad()));
        }
        out.push_str(&format!("{}]", self.pad()));
        out
    }

    fn map(self, json: &Value, value_info: Option<&TypeInfo>) -> String {
        let Some(object) = json.as_object().filter(|o| !o.is_empty()) else {
            return "{}".into();
        };
        let inner = self.inner();
        let mut out = "{\n".to_owned();
        for (key, value) in object {
            let rendered =
                value_info.map_or_else(|| inner.any(value), |info| inner.value(value, info));
            out.push_str(&format!("{}{}: {rendered},\n", inner.pad(), quote(key)));
        }
        out.push_str(&format!("{}}}", self.pad()));
        out
    }

    fn any(self, json: &Value) -> String {
        match json {
            Value::Null => "None".into(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) if n.is_f64() => float64(n.as_f64().unwrap_or_default()),
            Value::Number(n) => n.to_string(),
            Value::String(s) => quote(s),
            Value::Array(_) => self.list(json, None),
            Value::Object(_) => self.map(json, None),
        }
    }
}

fn primitive(json: &Value, p: Primitive) -> String {
    match (p, json) {
        (Primitive::F32, Value::Number(n)) => float32(n.as_f64().unwrap_or_default() as f32),
        (Primitive::F64, Value::Number(n)) => float64(n.as_f64().unwrap_or_default()),
        (_, Value::String(s)) => quote(s),
        (_, other) => other.to_string(),
    }
}

fn float32(x: f32) -> String {
    with_point(ryu::Buffer::new().format_finite(x))
}

fn float64(x: f64) -> String {
    with_point(ryu::Buffer::new().format_finite(x))
}

/// Ensures a `.` in the mantissa (`1e-7` → `1.0e-7`) so floats never read as integers.
fn with_point(text: &str) -> String {
    match text.split_once('e') {
        Some((mantissa, exp)) if !mantissa.contains('.') => format!("{mantissa}.0e{exp}"),
        _ if !text.contains(['.', 'e']) => format!("{text}.0"),
        _ => text.to_owned(),
    }
}

/// A RON string literal with minimal escaping.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
