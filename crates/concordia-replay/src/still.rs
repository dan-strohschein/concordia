//! Stills: the world's hashed component values at a checkpoint, as JSON in the DDC, so a
//! divergence can name fields and both values (decision R2: never restored).

use std::collections::BTreeMap;
use std::path::Path;

use concordia_core::ddc::{Ddc, DdcKey};
use concordia_terra::World;
use serde_json::Value;

use crate::methods::FieldDiff;

/// Component values by entity id, then component name (presentation-only components left out,
/// as in the world hash).
pub(crate) type Still = BTreeMap<String, BTreeMap<String, Value>>;

/// The most field differences a report lists.
pub(crate) const MAX_FIELDS: usize = 32;

/// `world`'s hashed component values.
pub(crate) fn take(world: &World) -> Result<Still, String> {
    let mut still = Still::new();
    for id in world.entity_ids_sorted() {
        let mut components = BTreeMap::new();
        for name in world.component_names(id).unwrap_or_default() {
            if world.is_presentation(name) {
                continue;
            }
            let value = world
                .get_dyn(id, name)
                .map_err(|e| e.to_string())?
                .to_json()
                .map_err(|e| e.to_string())?;
            components.insert(name.to_owned(), value);
        }
        still.insert(id.to_string(), components);
    }
    Ok(still)
}

/// Stores `still` in the DDC at `root`; returns its key.
pub(crate) fn store(root: &Path, still: &Still) -> Result<String, String> {
    let bytes = serde_json::to_vec(still).map_err(|e| e.to_string())?;
    let key = DdcKey::of(&[b"replay-still-v1", &bytes]);
    Ddc::new(root.to_path_buf())
        .put(&key, &bytes)
        .map_err(|e| e.to_string())?;
    Ok(key.to_string())
}

/// The still under `key` in the DDC at `root` (`None` if it is not cached).
pub(crate) fn load(root: &Path, key: &str) -> Option<Still> {
    let key: DdcKey = key.parse().ok()?;
    let bytes = Ddc::new(root.to_path_buf()).get(&key).ok()??;
    serde_json::from_slice(&bytes).ok()
}

/// The values that differ between `expected` and `actual`, entity by entity and component by
/// component, at most [`MAX_FIELDS`].
pub(crate) fn diff(expected: &Still, actual: &Still) -> Vec<FieldDiff> {
    let mut out = Vec::new();
    let ids: std::collections::BTreeSet<&String> = expected.keys().chain(actual.keys()).collect();
    for id in ids {
        match (expected.get(id), actual.get(id)) {
            (Some(a), Some(b)) => components(id, a, b, &mut out),
            (a, b) => out.push(field(id, "@entity", "", present(a), present(b))),
        }
        if out.len() >= MAX_FIELDS {
            break;
        }
    }
    out.truncate(MAX_FIELDS);
    out
}

fn present<T>(v: Option<&T>) -> Value {
    Value::Bool(v.is_some())
}

fn components(
    id: &str,
    a: &BTreeMap<String, Value>,
    b: &BTreeMap<String, Value>,
    out: &mut Vec<FieldDiff>,
) {
    let names: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    for name in names {
        match (a.get(name), b.get(name)) {
            (Some(x), Some(y)) => values(id, name, "", x, y, out),
            (x, y) => out.push(field(
                id,
                name,
                "",
                x.cloned().unwrap_or(Value::Null),
                y.cloned().unwrap_or(Value::Null),
            )),
        }
    }
}

/// Descends objects and arrays of the same shape; reports leaves (or shapes) that differ.
fn values(id: &str, component: &str, path: &str, a: &Value, b: &Value, out: &mut Vec<FieldDiff>) {
    if a == b || out.len() >= MAX_FIELDS {
        return;
    }
    let join = |key: &str| {
        if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}/{key}")
        }
    };
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for k in keys {
                let (vx, vy) = (
                    x.get(k).unwrap_or(&Value::Null),
                    y.get(k).unwrap_or(&Value::Null),
                );
                values(id, component, &join(k), vx, vy, out);
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (vx, vy)) in x.iter().zip(y).enumerate() {
                values(id, component, &join(&i.to_string()), vx, vy, out);
            }
        }
        _ => out.push(field(id, component, path, a.clone(), b.clone())),
    }
}

fn field(entity: &str, component: &str, path: &str, expected: Value, actual: Value) -> FieldDiff {
    FieldDiff {
        entity: entity.to_owned(),
        component: component.to_owned(),
        path: path.to_owned(),
        expected,
        actual,
    }
}
