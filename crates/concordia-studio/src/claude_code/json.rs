//! JSON that keeps object key order, so merging a user's settings never reorders them.

use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::StudioError;

/// A JSON value whose objects keep their key order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// An empty object.
    pub(crate) fn object() -> Self {
        Self::Object(Vec::new())
    }

    /// Parses `text` (empty text is an empty object).
    pub(crate) fn parse(text: &str, file: &str) -> Result<Self, StudioError> {
        if text.trim().is_empty() {
            return Ok(Self::object());
        }
        serde_json::from_str(text).map_err(|e| StudioError::Invalid {
            file: file.into(),
            message: format!("not valid JSON: {e}"),
        })
    }

    /// Pretty text with two-space indents and a final newline.
    pub(crate) fn to_text(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// The value at `key` of an object.
    pub(crate) fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The object at `key`, created (or replacing a non-object) if needed.
    pub(crate) fn entry(&mut self, key: &str) -> &mut Json {
        if !matches!(self, Self::Object(_)) {
            *self = Self::object();
        }
        let Self::Object(fields) = self else {
            unreachable!("made an object above")
        };
        let index = match fields.iter().position(|(k, _)| k == key) {
            Some(i) => i,
            None => {
                fields.push((key.to_owned(), Self::object()));
                fields.len() - 1
            }
        };
        &mut fields[index].1
    }

    /// Sets `key` in place (keeping its position) or appends it.
    pub(crate) fn set(&mut self, key: &str, value: Json) {
        *self.entry(key) = value;
    }

    /// Converts from a `serde_json::Value` (whose objects are sorted, which is fine for generated parts).
    pub(crate) fn from_value(value: serde_json::Value) -> Self {
        serde_json::from_value(value).unwrap_or(Self::Null)
    }
}

impl Serialize for Json {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => s.serialize_unit(),
            Self::Bool(b) => s.serialize_bool(*b),
            Self::Number(n) => n.serialize(s),
            Self::String(text) => s.serialize_str(text),
            Self::Array(items) => {
                let mut seq = s.serialize_seq(Some(items.len()))?;
                items
                    .iter()
                    .try_for_each(|item| seq.serialize_element(item))?;
                seq.end()
            }
            Self::Object(fields) => {
                let mut map = s.serialize_map(Some(fields.len()))?;
                fields
                    .iter()
                    .try_for_each(|(k, v)| map.serialize_entry(k, v))?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(JsonVisitor)
    }
}

struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Json;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JSON")
    }

    fn visit_unit<E>(self) -> Result<Json, E> {
        Ok(Json::Null)
    }

    fn visit_none<E>(self) -> Result<Json, E> {
        Ok(Json::Null)
    }

    fn visit_bool<E>(self, b: bool) -> Result<Json, E> {
        Ok(Json::Bool(b))
    }

    fn visit_i64<E>(self, n: i64) -> Result<Json, E> {
        Ok(Json::Number(n.into()))
    }

    fn visit_u64<E>(self, n: u64) -> Result<Json, E> {
        Ok(Json::Number(n.into()))
    }

    fn visit_f64<E>(self, n: f64) -> Result<Json, E> {
        Ok(serde_json::Number::from_f64(n).map_or(Json::Null, Json::Number))
    }

    fn visit_str<E>(self, text: &str) -> Result<Json, E> {
        Ok(Json::String(text.to_owned()))
    }

    fn visit_string<E>(self, text: String) -> Result<Json, E> {
        Ok(Json::String(text))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Json, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Json::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
        let mut fields = Vec::new();
        while let Some((key, value)) = map.next_entry::<String, Json>()? {
            fields.push((key, value));
        }
        Ok(Json::Object(fields))
    }
}
