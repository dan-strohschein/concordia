//! Reading `.scene.ron` and `.prefab.ron` documents.

use std::collections::{BTreeMap, HashSet};

use concordia_core::id::EntityId;
use concordia_core::suggest::did_you_mean;
use concordia_reflect::{DynReflect, ReflectError, TypeRegistration, TypeRegistry};
use concordia_terra::ReflectComponent;

use crate::convert::{ConvertError, Converter, found};
use crate::error::SceneError;
use crate::model::{EntityDoc, FORMAT_VERSION, PrefabDoc, SceneDoc};
use crate::syntax::{Body, Field, Kind, Node, Pos, SyntaxError, parse};

/// Reads a scene, validating every component against the registry.
pub fn read_scene(text: &str, registry: &TypeRegistry) -> Result<SceneDoc, SceneError> {
    let root = parse(text).map_err(syntax)?;
    let fields = Fields::of(&root, "Scene", &["format", "id", "name", "entities"])?;
    let format = fields.format()?;
    Ok(SceneDoc {
        format,
        id: fields.parsed("id")?,
        name: fields.string("name")?,
        entities: entities(fields.get("entities")?, registry)?,
    })
}

/// Reads a prefab, validating every component against the registry.
pub fn read_prefab(text: &str, registry: &TypeRegistry) -> Result<PrefabDoc, SceneError> {
    let root = parse(text).map_err(syntax)?;
    let fields = Fields::of(
        &root,
        "Prefab",
        &["format", "id", "name", "root", "entities"],
    )?;
    let format = fields.format()?;
    let prefab = PrefabDoc {
        format,
        id: fields.parsed("id")?,
        name: fields.string("name")?,
        root: fields.parsed("root")?,
        entities: entities(fields.get("entities")?, registry)?,
    };
    if !prefab.entities.iter().any(|e| e.id == prefab.root) {
        let pos = fields.get("root")?.pos;
        return Err(parse_error(
            pos,
            "root must be the id of one of the prefab's entities",
        ));
    }
    Ok(prefab)
}

/// The named fields of a `Name(...)` document node.
struct Fields<'a> {
    node: &'a Node,
    fields: &'a [Field],
}

impl<'a> Fields<'a> {
    fn of(node: &'a Node, kind: &str, allowed: &[&str]) -> Result<Self, SceneError> {
        let fields = match &node.kind {
            Kind::Call {
                name: Some(name),
                body: Body::Fields(fields),
            } if name == kind => fields.as_slice(),
            _ => {
                return Err(parse_error(
                    node.pos,
                    &format!("expected {kind}(...) with named fields"),
                ));
            }
        };
        let mut seen = HashSet::new();
        for field in fields {
            if !allowed.contains(&field.name.as_str()) {
                return Err(parse_error(
                    field.pos,
                    &format!(
                        "unknown field `{}`; expected {}",
                        field.name,
                        allowed.join(", ")
                    ),
                ));
            }
            if !seen.insert(field.name.as_str()) {
                return Err(parse_error(
                    field.pos,
                    &format!("field `{}` appears twice", field.name),
                ));
            }
        }
        Ok(Self { node, fields })
    }

    fn get(&self, name: &str) -> Result<&'a Node, SceneError> {
        self.fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| &f.value)
            .ok_or_else(|| parse_error(self.node.pos, &format!("missing field `{name}`")))
    }

    fn string(&self, name: &str) -> Result<String, SceneError> {
        let node = self.get(name)?;
        match &node.kind {
            Kind::Str(s) => Ok(s.clone()),
            _ => Err(parse_error(
                node.pos,
                &format!("`{name}` must be a string, found {}", found(node)),
            )),
        }
    }

    fn parsed<T: std::str::FromStr<Err: std::fmt::Display>>(
        &self,
        name: &str,
    ) -> Result<T, SceneError> {
        let node = self.get(name)?;
        let text = self.string(name)?;
        text.parse()
            .map_err(|e| parse_error(node.pos, &format!("invalid `{name}`: {e}")))
    }

    fn format(&self) -> Result<u32, SceneError> {
        let node = self.get("format")?;
        let version = match &node.kind {
            Kind::Num(n) => n.parse::<u64>().ok(),
            _ => None,
        };
        match version {
            Some(v) if v == u64::from(FORMAT_VERSION) => Ok(FORMAT_VERSION),
            Some(v) if v > u64::from(FORMAT_VERSION) => Err(SceneError::UnsupportedFormat {
                found: v,
                supported: FORMAT_VERSION,
            }),
            _ => Err(parse_error(
                node.pos,
                &format!("`format` must be {FORMAT_VERSION}"),
            )),
        }
    }
}

fn entities(node: &Node, registry: &TypeRegistry) -> Result<Vec<EntityDoc>, SceneError> {
    let Kind::List(items) = &node.kind else {
        return Err(parse_error(
            node.pos,
            "`entities` must be a list [Entity(...), ...]",
        ));
    };
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let fields = Fields::of(item, "Entity", &["id", "components"])?;
        let id: EntityId = fields.parsed("id")?;
        if !seen.insert(id) {
            let pos = fields.get("id")?.pos;
            return Err(SceneError::DuplicateEntity {
                id,
                line: pos.line,
                col: pos.col,
            });
        }
        out.push(EntityDoc {
            id,
            components: components(fields.get("components")?, registry)?,
        });
    }
    Ok(out)
}

fn components(
    node: &Node,
    registry: &TypeRegistry,
) -> Result<BTreeMap<String, Box<dyn DynReflect>>, SceneError> {
    let Kind::Map(entries) = &node.kind else {
        return Err(parse_error(
            node.pos,
            "`components` must be a map {\"Name\": value, ...}",
        ));
    };
    let mut out = BTreeMap::new();
    for (key, value) in entries {
        let Kind::Str(name) = &key.kind else {
            return Err(parse_error(key.pos, "component names must be strings"));
        };
        if out.contains_key(name) {
            return Err(parse_error(
                key.pos,
                &format!("component {name:?} appears twice"),
            ));
        }
        let registration = component(registry, name, key.pos)?;
        out.insert(name.clone(), component_value(registration, name, value)?);
    }
    Ok(out)
}

fn component<'r>(
    registry: &'r TypeRegistry,
    name: &str,
    pos: Pos,
) -> Result<&'r TypeRegistration, SceneError> {
    let is_component = |r: &&TypeRegistration| r.data::<ReflectComponent>().is_some();
    registry
        .get_by_name(name)
        .filter(is_component)
        .ok_or_else(|| {
            let names = registry
                .names()
                .filter(|n| registry.get_by_name(n).is_some_and(|r| is_component(&r)));
            SceneError::UnknownComponent {
                name: name.to_owned(),
                line: pos.line,
                col: pos.col,
                did_you_mean: did_you_mean(name, names).first().map(|s| (*s).to_owned()),
            }
        })
}

fn component_value(
    registration: &TypeRegistration,
    name: &str,
    node: &Node,
) -> Result<Box<dyn DynReflect>, SceneError> {
    let mut converter = Converter::default();
    let json = converter
        .value(node, registration.info(), "")
        .map_err(|e| convert_error(name, e))?;
    registration
        .from_json(json)
        .map_err(|e| reflect_error(name, &e, &converter))
}

pub(crate) fn convert_error(component: &str, e: ConvertError) -> SceneError {
    SceneError::InvalidValue {
        component: component.to_owned(),
        pointer: e.pointer,
        expected: e.expected,
        found: e.found,
        line: e.pos.line,
        col: e.pos.col,
    }
}

pub(crate) fn reflect_error(
    component: &str,
    e: &ReflectError,
    converter: &Converter,
) -> SceneError {
    let (pointer, at, expected, found) = match e {
        ReflectError::Invalid {
            pointer,
            expected,
            found,
        } => (
            pointer.clone(),
            pointer.clone(),
            expected.clone(),
            found.clone(),
        ),
        ReflectError::MissingField { pointer, field } => (
            pointer.clone(),
            pointer.clone(),
            format!("field `{field}`"),
            "nothing".into(),
        ),
        ReflectError::UnknownField {
            pointer,
            field,
            expected,
        } => (
            pointer.clone(),
            format!("{pointer}/{field}"),
            format!("one of the fields {}", sorted(expected).join(", ")),
            format!("field `{field}`"),
        ),
        ReflectError::OutOfRange {
            pointer,
            value,
            min,
            max,
        } => (
            pointer.clone(),
            pointer.clone(),
            format!("a value in [{min}, {max}]"),
            value.to_string(),
        ),
        other => (
            String::new(),
            String::new(),
            "a valid value".into(),
            other.to_string(),
        ),
    };
    let pos = converter.pos_of(&at);
    SceneError::InvalidValue {
        component: component.to_owned(),
        pointer,
        expected,
        found,
        line: pos.line,
        col: pos.col,
    }
}

/// Field names in canonical (sorted) order.
fn sorted(names: &[&'static str]) -> Vec<&'static str> {
    let mut names = names.to_vec();
    names.sort_unstable();
    names
}

pub(crate) fn syntax(e: SyntaxError) -> SceneError {
    parse_error(e.pos, &e.message)
}

fn parse_error(pos: Pos, message: &str) -> SceneError {
    SceneError::Parse {
        line: pos.line,
        col: pos.col,
        message: message.to_owned(),
    }
}
