//! `entity.spawn`, `entity.despawn`, `entity.get`.

use std::collections::BTreeMap;

use concordia_command::{Actor, EditOp};
use concordia_core::id::{EntityId, IdSource};
use concordia_ecs::component_from_json;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Empty, EntityRef, WorldSel, edit, require_component, resolve, world_of};
use crate::engine::{Engine, internal};
use crate::error::ApiError;
use crate::map::{self, Hints};
use crate::method::{CallCtx, Method, MethodInfo};

/// Create an entity with components (values as JSON; see concordia://schema/component/{name}).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EntitySpawn;

/// `entity.spawn` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct EntitySpawnParams {
    /// Component values by name, e.g. {"Name": "Player", "Transform": {...}}.
    pub components: BTreeMap<String, Value>,
    /// Use this id instead of a fresh one.
    pub id: Option<EntityId>,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// `entity.spawn` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EntitySpawnResult {
    /// The entity's id.
    pub id: EntityId,
}

/// Delete an entity and all its components (undoable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EntityDespawn;

/// `entity.despawn` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct EntityDespawnParams {
    /// The entity.
    pub id: EntityRef,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// Read an entity's components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EntityGet;

/// `entity.get` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct EntityGetParams {
    /// The entity.
    pub id: EntityRef,
    /// "edit" (default) or "play".
    pub world: Option<WorldSel>,
}

/// `entity.get` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EntityGetResult {
    /// The entity.
    pub id: EntityId,
    /// Component values by name.
    pub components: BTreeMap<String, Value>,
}

impl Method for EntitySpawn {
    const NAME: &'static str = "entity.spawn";
    const MUTATES: bool = true;
    type Params = EntitySpawnParams;
    type Result = EntitySpawnResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: EntitySpawnParams,
    ) -> Result<EntitySpawnResult, ApiError> {
        engine.scene()?;
        for (name, value) in &p.components {
            require_component(engine, name)?;
            component_from_json(&engine.registry, name, value.clone()).map_err(|e| match e {
                concordia_ecs::EcsError::Reflect(r) => {
                    map::value_invalid(name, &r, &engine.registry)
                        .at(format!("/components/{name}{}", pointer(&r)))
                }
                other => map::ecs(
                    other,
                    &Hints {
                        registry: &engine.registry,
                        world: None,
                    },
                ),
            })?;
        }
        let id = match p.id {
            Some(id) => id,
            None => engine.ids.next_id().map_err(internal)?,
        };
        edit(
            engine,
            ctx,
            Self::NAME,
            EditOp::Spawn {
                id,
                components: p.components,
            },
            None,
        )?;
        Ok(EntitySpawnResult { id })
    }
}

impl Method for EntityDespawn {
    const NAME: &'static str = "entity.despawn";
    const MUTATES: bool = true;
    type Params = EntityDespawnParams;
    type Result = Empty;

    fn call(engine: &mut Engine, ctx: &CallCtx, p: EntityDespawnParams) -> Result<Empty, ApiError> {
        let id = resolve(&engine.scene()?.world, &p.id)?;
        edit(engine, ctx, Self::NAME, EditOp::Despawn { id }, None)?;
        Ok(Empty {})
    }
}

impl Method for EntityGet {
    const NAME: &'static str = "entity.get";
    const MUTATES: bool = false;
    type Params = EntityGetParams;
    type Result = EntityGetResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: EntityGetParams,
    ) -> Result<EntityGetResult, ApiError> {
        let world = world_of(engine, p.world)?;
        let id = resolve(world, &p.id)?;
        let components = world
            .component_names(id)
            .unwrap_or_default()
            .into_iter()
            .map(|name| Ok((name.to_owned(), world.get_json(id, name).map_err(internal)?)))
            .collect::<Result<_, ApiError>>()?;
        Ok(EntityGetResult { id, components })
    }
}

/// The JSON Pointer inside a component value that a reflect error refers to.
pub(crate) fn pointer(e: &concordia_reflect::ReflectError) -> String {
    use concordia_reflect::ReflectError as E;
    match e {
        E::Invalid { pointer, .. }
        | E::MissingField { pointer, .. }
        | E::UnknownField { pointer, .. }
        | E::OutOfRange { pointer, .. } => pointer.clone(),
        _ => String::new(),
    }
}

pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<EntitySpawn>(),
        MethodInfo::of::<EntityDespawn>(),
        MethodInfo::of::<EntityGet>(),
    ]
}
