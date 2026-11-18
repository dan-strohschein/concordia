//! Prefab instantiation: flattened copies with fresh ids (no instance links in M0).

use std::collections::{BTreeMap, HashMap};

use concordia_command::EditOp;
use concordia_core::id::{EntityId, IdSource};
use serde_json::Value;

use crate::error::SceneError;
use crate::model::PrefabDoc;

/// Spawn ops for a fresh copy of `prefab` (root first), with every reference to a prefab entity remapped.
pub fn instantiate_prefab(
    prefab: &PrefabDoc,
    ids: &mut impl IdSource,
) -> Result<(EntityId, Vec<EditOp>), SceneError> {
    let mut order: Vec<_> = prefab.entities.iter().collect();
    order.sort_by_key(|e| (e.id != prefab.root, e.id));
    let mapping: HashMap<String, EntityId> = order
        .iter()
        .map(|e| Ok((e.id.to_string(), ids.next_id()?)))
        .collect::<Result<_, SceneError>>()?;
    let ops = order
        .iter()
        .map(|entity| {
            let components = entity
                .components
                .iter()
                .map(|(name, value)| Ok((name.clone(), remap(value.to_json()?, &mapping))))
                .collect::<Result<BTreeMap<_, _>, SceneError>>()?;
            Ok(EditOp::Spawn {
                id: mapping[&entity.id.to_string()],
                components,
            })
        })
        .collect::<Result<Vec<_>, SceneError>>()?;
    Ok((mapping[&prefab.root.to_string()], ops))
}

/// Replaces every string equal to an old id with the new id.
fn remap(value: Value, mapping: &HashMap<String, EntityId>) -> Value {
    match value {
        Value::String(s) => match mapping.get(&s) {
            Some(new) => Value::String(new.to_string()),
            None => Value::String(s),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(|v| remap(v, mapping)).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, remap(v, mapping)))
                .collect(),
        ),
        other => other,
    }
}
