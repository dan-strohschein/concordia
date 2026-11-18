//! Moving documents into and out of worlds.

use std::collections::BTreeMap;

use concordia_reflect::TypeRegistry;
use concordia_terra::World;

use crate::error::SceneError;
use crate::model::{EntityDoc, FORMAT_VERSION, SceneDoc, SceneMeta};

/// Spawns every entity of `doc` into `world` (loading is not an edit; it is not journaled).
pub fn load_into_world(
    doc: &SceneDoc,
    world: &mut World,
    registry: &TypeRegistry,
) -> Result<(), SceneError> {
    for entity in &doc.entities {
        world.spawn_with_id(entity.id)?;
        for value in entity.components.values() {
            world.insert_dyn(entity.id, value.clone_dyn(), registry)?;
        }
    }
    Ok(())
}

/// Captures `world` as a scene document, entities sorted by id.
pub fn snapshot_world(meta: &SceneMeta, world: &World) -> Result<SceneDoc, SceneError> {
    let entities = world
        .entity_ids_sorted()
        .into_iter()
        .map(|id| {
            let names = world.component_names(id).unwrap_or_default();
            let components = names
                .into_iter()
                .map(|name| Ok((name.to_owned(), world.get_dyn(id, name)?.clone_dyn())))
                .collect::<Result<BTreeMap<_, _>, SceneError>>()?;
            Ok(EntityDoc { id, components })
        })
        .collect::<Result<Vec<_>, SceneError>>()?;
    Ok(SceneDoc {
        format: FORMAT_VERSION,
        id: meta.id,
        name: meta.name.clone(),
        entities,
    })
}
