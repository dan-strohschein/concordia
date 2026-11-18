//! Slang's `-reflection-json` output, reduced to what pipelines and layout checks need.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ShaderError;

/// A global parameter and its slot (Metal buffer index).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    /// Parameter name.
    pub name: String,
    /// Slot index.
    pub index: u32,
}

/// One field of a reflected struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldLayout {
    /// Name.
    pub name: String,
    /// Byte offset.
    pub offset: usize,
    /// Byte size.
    pub size: usize,
}

/// A reflected struct (e.g. the push-constant block).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructLayout {
    /// Slang type name.
    pub name: String,
    /// Total size in bytes.
    pub size: usize,
    /// Fields in declaration order.
    pub fields: Vec<FieldLayout>,
}

/// What a compiled entry point exposes.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ShaderReflection {
    /// Compute thread group size.
    pub threads_per_group: Option<[u32; 3]>,
    /// Global parameters by slot.
    pub bindings: Vec<Binding>,
    /// The entry point's `uniform push` parameter layout, if any (bound at slot 3).
    pub push: Option<StructLayout>,
}

/// Parses reflection JSON for `entry`.
pub(crate) fn parse(json: &str, entry: &str) -> Result<ShaderReflection, ShaderError> {
    let root: Value =
        serde_json::from_str(json).map_err(|e| ShaderError::Reflection(e.to_string()))?;
    let params = root["parameters"].as_array().cloned().unwrap_or_default();
    let mut bindings: Vec<Binding> = params
        .iter()
        .filter_map(|p| {
            Some(Binding {
                name: p["name"].as_str()?.to_owned(),
                index: u32::try_from(p["binding"]["index"].as_u64()?).ok()?,
            })
        })
        .collect();
    bindings.sort_by_key(|b| b.index);
    let entry_point = root["entryPoints"]
        .as_array()
        .and_then(|eps| eps.iter().find(|e| e["name"] == entry));
    let push = entry_point
        .and_then(|e| e["scope"]["parameters"].as_array())
        .and_then(|ps| ps.iter().find(|p| p["name"] == "push"))
        .map(|p| struct_layout(&p["type"]))
        .transpose()?;
    let threads_per_group = entry_point
        .and_then(|e| e["threadGroupSize"].as_array())
        .and_then(|t| {
            Some([
                t.first()?.as_u64()? as u32,
                t.get(1)?.as_u64()? as u32,
                t.get(2)?.as_u64()? as u32,
            ])
        });
    Ok(ShaderReflection {
        threads_per_group,
        bindings,
        push,
    })
}

fn struct_layout(ty: &Value) -> Result<StructLayout, ShaderError> {
    let bad = || ShaderError::Reflection(format!("push is not a struct with uniform layout: {ty}"));
    let fields = ty["fields"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|f| {
            Some(FieldLayout {
                name: f["name"].as_str()?.to_owned(),
                offset: f["binding"]["offset"].as_u64()? as usize,
                size: f["binding"]["size"].as_u64()? as usize,
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(bad)?;
    let size = ty["sizes"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["kind"] == "uniform"))
        .and_then(|s| s["value"].as_u64())
        .map(|v| v as usize)
        .or_else(|| fields.iter().map(|f| f.offset + f.size).max())
        .ok_or_else(bad)?;
    Ok(StructLayout {
        name: ty["name"].as_str().unwrap_or_default().to_owned(),
        size,
        fields,
    })
}
