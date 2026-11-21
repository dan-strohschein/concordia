//! `component.get`, `component.add`, `component.set`, `component.patch`, `component.remove`.

use concordia_command::{Actor, EditOp};
use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use concordia_terra::{EcsError, World};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{EntityRef, WorldSel, edit, require_component, resolve, world_of};
use crate::engine::Engine;
use crate::error::{ApiError, ApiErrorKind};
use crate::map;
use crate::method::{CallCtx, Method, MethodInfo};
use crate::patch::merge_patch;

/// Read one component value as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ComponentGet;

/// `component.get` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ComponentGetParams {
    /// The entity.
    pub entity: EntityRef,
    /// Component name, e.g. "Transform".
    pub component: String,
    /// "edit" (default) or "play".
    pub world: Option<WorldSel>,
}

/// A component value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ValueResult {
    /// The value.
    pub value: Value,
}

/// Add a component with its default value (see the `components` list of schema.get for what is addable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ComponentAdd;

/// `component.add` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ComponentAddParams {
    /// The entity (it must not have the component yet).
    pub entity: EntityRef,
    /// Component name, e.g. "PointLight".
    pub component: String,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// Insert or replace a component (the whole value; use component.patch to change some fields). With world "play", writes the running simulation (not undoable; the next tick sees it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ComponentSet;

/// `component.set` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ComponentSetParams {
    /// The entity.
    pub entity: EntityRef,
    /// Component name.
    pub component: String,
    /// The complete new value (see concordia://schema/component/{name}).
    pub value: Value,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
    /// "edit" (default, journaled) or "play" (the running simulation, not journaled).
    pub world: Option<WorldSel>,
}

/// `component.set` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SetResult {
    /// The value before, if the entity had the component.
    pub previous: Option<Value>,
}

/// Change some fields of a component with an RFC 7396 merge patch, e.g. {"scale": [2, 2, 2]}. With world "play", patches the running simulation (not undoable; the next tick sees it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ComponentPatch;

/// `component.patch` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ComponentPatchParams {
    /// The entity.
    pub entity: EntityRef,
    /// Component name (the entity must have it).
    pub component: String,
    /// Merge patch: objects merge, null deletes a field, anything else replaces.
    pub patch: Value,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
    /// "edit" (default, journaled) or "play" (the running simulation, not journaled).
    pub world: Option<WorldSel>,
}

/// Remove a component from an entity (undoable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ComponentRemove;

/// `component.remove` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ComponentRemoveParams {
    /// The entity.
    pub entity: EntityRef,
    /// Component name.
    pub component: String,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// `component.remove` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RemoveResult {
    /// The removed value.
    pub previous: Value,
}

impl Method for ComponentGet {
    const NAME: &'static str = "component.get";
    const MUTATES: bool = false;
    type Params = ComponentGetParams;
    type Result = ValueResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: ComponentGetParams,
    ) -> Result<ValueResult, ApiError> {
        require_component(engine, &p.component)?;
        let world = world_of(engine, p.world)?;
        let id = resolve(world, &p.entity)?;
        Ok(ValueResult {
            value: current(world, id, &p.component)?,
        })
    }
}

impl Method for ComponentAdd {
    const NAME: &'static str = "component.add";
    const MUTATES: bool = true;
    type Params = ComponentAddParams;
    type Result = ValueResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: ComponentAddParams,
    ) -> Result<ValueResult, ApiError> {
        require_component(engine, &p.component)?;
        let value = default_of(engine, &p.component)?;
        let world = &engine.scene()?.world;
        let id = resolve(world, &p.entity)?;
        if world.get_json(id, &p.component).is_ok() {
            return Err(already_has(id, &p.component));
        }
        let op = EditOp::SetComponent {
            id,
            component: p.component.clone(),
            value,
        };
        edit(engine, ctx, Self::NAME, op, Some(&p.component))?;
        Ok(ValueResult {
            value: current(&engine.scene()?.world, id, &p.component)?,
        })
    }
}

impl Method for ComponentSet {
    const NAME: &'static str = "component.set";
    const MUTATES: bool = true;
    type Params = ComponentSetParams;
    type Result = SetResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: ComponentSetParams,
    ) -> Result<SetResult, ApiError> {
        require_component(engine, &p.component)?;
        if p.world == Some(WorldSel::play) {
            let previous = write_play(engine, ctx, Self::NAME, &p.entity, &p.component, |_| {
                Some(p.value)
            })?;
            return Ok(SetResult { previous });
        }
        let world = &engine.scene()?.world;
        let id = resolve(world, &p.entity)?;
        let previous = world.get_json(id, &p.component).ok();
        let op = EditOp::SetComponent {
            id,
            component: p.component.clone(),
            value: p.value,
        };
        edit(engine, ctx, Self::NAME, op, Some(&p.component))?;
        Ok(SetResult { previous })
    }
}

impl Method for ComponentPatch {
    const NAME: &'static str = "component.patch";
    const MUTATES: bool = true;
    type Params = ComponentPatchParams;
    type Result = ValueResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: ComponentPatchParams,
    ) -> Result<ValueResult, ApiError> {
        require_component(engine, &p.component)?;
        if p.world == Some(WorldSel::play) {
            let patch = |old: Option<Value>| {
                let mut value = old?;
                merge_patch(&mut value, &p.patch);
                Some(value)
            };
            write_play(engine, ctx, Self::NAME, &p.entity, &p.component, patch)?;
            let world = world_of(engine, p.world)?;
            let id = resolve(world, &p.entity)?;
            return Ok(ValueResult {
                value: current(world, id, &p.component)?,
            });
        }
        let world = &engine.scene()?.world;
        let id = resolve(world, &p.entity)?;
        let mut value = current(world, id, &p.component)?;
        merge_patch(&mut value, &p.patch);
        let op = EditOp::SetComponent {
            id,
            component: p.component.clone(),
            value,
        };
        edit(engine, ctx, Self::NAME, op, Some(&p.component))?;
        Ok(ValueResult {
            value: current(&engine.scene()?.world, id, &p.component)?,
        })
    }
}

impl Method for ComponentRemove {
    const NAME: &'static str = "component.remove";
    const MUTATES: bool = true;
    type Params = ComponentRemoveParams;
    type Result = RemoveResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: ComponentRemoveParams,
    ) -> Result<RemoveResult, ApiError> {
        require_component(engine, &p.component)?;
        let world = &engine.scene()?.world;
        let id = resolve(world, &p.entity)?;
        let previous = current(world, id, &p.component)?;
        edit(
            engine,
            ctx,
            Self::NAME,
            EditOp::RemoveComponent {
                id,
                component: p.component,
            },
            None,
        )?;
        Ok(RemoveResult { previous })
    }
}

/// Writes `component` of `entity` in the play world (ADR-0075): `value` maps the current value
/// to the new one (`None`: the entity lacks a component it needs); logged with the actor, never
/// journaled. Returns the previous value.
fn write_play(
    engine: &mut Engine,
    ctx: &CallCtx,
    method: &str,
    entity: &EntityRef,
    component: &str,
    value: impl FnOnce(Option<Value>) -> Option<Value>,
) -> Result<Option<Value>, ApiError> {
    let actor = ctx.actor()?.to_string();
    let (registry, play) = engine.registry_and_play_mut();
    let world = play.ok_or_else(|| ApiError::from_kind(ApiErrorKind::NotPlaying))?;
    let id = resolve(world, entity)?;
    let old = world.get_json(id, component).ok();
    let new = value(old).ok_or_else(|| missing(world, id, component))?;
    let previous = world
        .set_json(id, component, new, registry)
        .map_err(|e| match e {
            EcsError::Reflect(r) => map::value_invalid(component, &r, registry),
            other => map::ecs(
                other,
                &map::Hints {
                    registry,
                    world: None,
                },
            ),
        })?;
    tracing::info!(target: "concordia::api", actor, method, entity = %id, component, "play world changed");
    Ok(previous)
}

/// The registered default of `component`, or an error saying how to add it with a value.
fn default_of(engine: &Engine, component: &str) -> Result<Value, ApiError> {
    let info = engine
        .registry
        .get_by_name(component)
        .map(|r| r.info())
        .ok_or_else(|| crate::engine::internal(format!("{component} vanished")))?;
    info.default.map(|default| default.get()).ok_or_else(|| {
        ApiError::new(
            ApiErrorKind::ValueInvalid,
            format!("{component} has no default value"),
        )
        .suggest([format!(
            "add it with component.set and a value; see schema.get {{\"name\": \"{component}\"}}"
        )])
        .see(format!("concordia://schema/component/{component}"))
    })
}

fn already_has(id: EntityId, component: &str) -> ApiError {
    ApiError::new(
        ApiErrorKind::ValueInvalid,
        format!("entity {id} already has a {component} component"),
    )
    .suggest(["change it with component.patch or replace it with component.set".to_owned()])
}

/// The component's current JSON value, or an error naming what the entity does have.
fn current(world: &World, id: EntityId, component: &str) -> Result<Value, ApiError> {
    world
        .get_json(id, component)
        .map_err(|_| missing(world, id, component))
}

/// The entity has no `component`: the error names what it does have.
fn missing(world: &World, id: EntityId, component: &str) -> ApiError {
    let has = world.component_names(id).unwrap_or_default().join(", ");
    ApiError::new(
        ApiErrorKind::ValueInvalid,
        format!("entity {id} has no {component} component"),
    )
    .suggest([format!("it has: {has}; add it with component.set")])
}

pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<ComponentGet>(),
        MethodInfo::of::<ComponentAdd>(),
        MethodInfo::of::<ComponentSet>(),
        MethodInfo::of::<ComponentPatch>(),
        MethodInfo::of::<ComponentRemove>(),
    ]
}
