//! Reading and writing `.template.ron` documents (M2 phase 2.0c): traits by name with their
//! parameters, and component values by name, both typed by the registry.

use std::collections::BTreeMap;

use concordia_core::suggest::did_you_mean;
use concordia_reflect::{DynReflect, TypeRegistry};

use crate::error::SceneError;
use crate::model::{FORMAT_VERSION, TemplateDoc};
use crate::read::{Fields, component_value, components, parse_error, syntax};
use crate::syntax::{Kind, Node, parse};
use crate::write::{header, named_values, quote};

const FIELDS: [&str; 4] = ["format", "description", "traits", "components"];

/// Reads a template, validating its trait parameters and components against the registry.
pub fn read_template(text: &str, registry: &TypeRegistry) -> Result<TemplateDoc, SceneError> {
    let root = parse(text).map_err(syntax)?;
    let fields = Fields::of(&root, "Template", &FIELDS)?;
    Ok(TemplateDoc {
        format: fields.format()?,
        description: fields.string("description")?,
        traits: typed_values(fields.get("traits")?, registry)?,
        components: components(fields.get("components")?, registry)?,
    })
}

/// Writes a template in canonical form.
pub fn write_template(doc: &TemplateDoc) -> Result<String, SceneError> {
    let mut out = header("template");
    out.push_str("Template(\n");
    out.push_str(&format!(
        "    format: {FORMAT_VERSION},\n    description: {},\n",
        quote(&doc.description)
    ));
    named_values(&mut out, "traits", &doc.traits, "template trait")?;
    named_values(
        &mut out,
        "components",
        &doc.components,
        "template component",
    )?;
    out.push_str(")\n");
    Ok(out)
}

/// A `{"TypeName": value, ...}` map whose values are any registered type.
fn typed_values(
    node: &Node,
    registry: &TypeRegistry,
) -> Result<BTreeMap<String, Box<dyn DynReflect>>, SceneError> {
    let Kind::Map(entries) = &node.kind else {
        return Err(parse_error(
            node.pos,
            "`traits` must be a map {\"Name\": (params), ...}",
        ));
    };
    let mut out = BTreeMap::new();
    for (key, value) in entries {
        let Kind::Str(name) = &key.kind else {
            return Err(parse_error(key.pos, "trait names must be strings"));
        };
        if out.contains_key(name) {
            return Err(parse_error(
                key.pos,
                &format!("trait {name:?} appears twice"),
            ));
        }
        let registration = registry
            .get_by_name(name)
            .ok_or_else(|| SceneError::UnknownType {
                name: name.clone(),
                line: key.pos.line,
                col: key.pos.col,
                did_you_mean: did_you_mean(name, registry.names())
                    .first()
                    .map(|s| (*s).to_owned()),
            })?;
        out.insert(name.clone(), component_value(registration, name, value)?);
    }
    Ok(out)
}
