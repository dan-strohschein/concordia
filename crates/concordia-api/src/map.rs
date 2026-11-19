//! Converting engine errors into agent-actionable API errors.

use concordia_command::CommandError;
use concordia_core::suggest::did_you_mean;
use concordia_reflect::{ReflectError, TypeInfo, TypeRegistry, example_json, schema_for};
use concordia_scene::SceneError;
use concordia_terra::{EcsError, ReflectComponent, World};
use serde_json::{Value, json};

use crate::error::{ApiError, ApiErrorKind};

/// Context for building suggestions.
pub(crate) struct Hints<'a> {
    pub(crate) registry: &'a TypeRegistry,
    pub(crate) world: Option<&'a World>,
}

/// Params failed schema validation.
pub(crate) fn invalid_params(e: &ReflectError, info: &'static TypeInfo, method: &str) -> ApiError {
    let schema = schema_for(info);
    let pointer = pointer_of(e);
    let mut err = ApiError::new(
        ApiErrorKind::InvalidParams,
        format!("invalid params for {method}: {e}"),
    )
    .see(format!("concordia://schema/method/{method}"));
    err.data.expected = Some(schema_at(&schema, &pointer));
    err.data.example = Some(example_json(info));
    err.at(pointer)
}

/// A component name that is not a registered component.
pub(crate) fn component_not_registered(name: &str, registry: &TypeRegistry) -> ApiError {
    let names = component_names(registry);
    let similar = did_you_mean(name, names.iter().copied())
        .into_iter()
        .map(|n| format!("did you mean \"{n}\"?"));
    ApiError::new(
        ApiErrorKind::ComponentNotRegistered,
        format!("no component named {name:?} is registered"),
    )
    .suggest(similar)
    .see("concordia://schema/index")
}

/// Registered component names, sorted.
pub(crate) fn component_names(registry: &TypeRegistry) -> Vec<&'static str> {
    registry
        .names()
        .filter(|n| {
            registry
                .get_by_name(n)
                .is_some_and(|r| r.data::<ReflectComponent>().is_some())
        })
        .collect()
}

/// A component value failed validation.
pub(crate) fn value_invalid(
    component: &str,
    e: &ReflectError,
    registry: &TypeRegistry,
) -> ApiError {
    let pointer = pointer_of(e);
    let mut err = ApiError::new(
        ApiErrorKind::ValueInvalid,
        format!("invalid {component} value: {e}"),
    )
    .see(format!("concordia://schema/component/{component}"));
    if let Some(registration) = registry.get_by_name(component) {
        let schema = schema_for(registration.info());
        err.data.expected = Some(schema_at(&schema, &pointer));
        err.data.example = Some(example_json(registration.info()));
    }
    err.at(pointer)
}

/// No entity has `given` (an id, or a string that is not an id).
pub(crate) fn entity_not_found(given: &str, world: Option<&World>) -> ApiError {
    let mut suggestions = Vec::new();
    if let Some(world) = world {
        let named = named_entities(world);
        if let Some((id, _)) = named.iter().find(|(_, name)| name == given) {
            suggestions.push(format!(
                "the entity named {given:?} has id {id}; pass that id"
            ));
        }
        let names: Vec<&str> = named.iter().map(|(_, n)| n.as_str()).collect();
        for similar in did_you_mean(given, names) {
            if let Some((id, _)) = named.iter().find(|(_, n)| n == similar) {
                suggestions.push(format!(
                    "did you mean the entity named {similar:?} (id {id})?"
                ));
            }
        }
    }
    ApiError::new(
        ApiErrorKind::EntityNotFound,
        format!("no entity with id {given:?}"),
    )
    .suggest(suggestions)
    .see("concordia://docs/agent-guide")
}

fn named_entities(world: &World) -> Vec<(String, String)> {
    world
        .entity_ids_sorted()
        .into_iter()
        .filter_map(|id| {
            let name = world.get_json(id, "Name").ok()?;
            Some((id.to_string(), name.as_str()?.to_owned()))
        })
        .collect()
}

/// Maps an ECS error.
pub(crate) fn ecs(e: EcsError, hints: &Hints<'_>) -> ApiError {
    match e {
        EcsError::NoSuchEntity(id) => entity_not_found(&id.to_string(), hints.world),
        EcsError::ComponentNotRegistered { name } | EcsError::NotAComponent { name } => {
            component_not_registered(&name, hints.registry)
        }
        EcsError::ComponentMissing { entity, component } => ApiError::new(
            ApiErrorKind::ValueInvalid,
            format!("entity {entity} has no {component} component"),
        )
        .suggest([format!(
            "use entity.get {{\"id\": \"{entity}\"}} to list its components"
        )]),
        EcsError::DuplicateId(id) => ApiError::new(
            ApiErrorKind::ValueInvalid,
            format!("entity {id} already exists"),
        )
        .suggest(["omit \"id\" to get a fresh one".to_owned()])
        .at("/id"),
        EcsError::Reflect(ReflectError::UnknownType { name }) => {
            component_not_registered(&name, hints.registry)
        }
        EcsError::Reflect(e) => {
            ApiError::new(ApiErrorKind::ValueInvalid, e.to_string()).at(pointer_of(&e))
        }
        other => crate::engine::internal(other),
    }
}

/// Maps a command error; `component` names the component the op touched (for value errors).
pub(crate) fn command(e: CommandError, component: Option<&str>, hints: &Hints<'_>) -> ApiError {
    match e {
        CommandError::OpFailed {
            source: EcsError::Reflect(r),
            ..
        } if component.is_some() => {
            value_invalid(component.unwrap_or_default(), &r, hints.registry)
        }
        CommandError::OpFailed { source, .. } | CommandError::Ecs(source) => ecs(source, hints),
        CommandError::AlreadyOpen { txn, actor, label } => ApiError::new(
            ApiErrorKind::TxnConflict,
            format!("transaction {txn} ({label:?}) by {actor} is open"),
        )
        .suggest(["commit or abort it with txn.commit / txn.abort first".to_owned()]),
        CommandError::NoOpenTxn => ApiError::from_kind(ApiErrorKind::TxnNotFound),
        CommandError::NothingToUndo { reason } => ApiError::new(
            ApiErrorKind::NothingToUndo,
            format!("nothing to undo: {reason}"),
        ),
        CommandError::NothingToRedo => ApiError::from_kind(ApiErrorKind::NothingToRedo),
        CommandError::NotUndoable { .. } => ApiError::new(ApiErrorKind::TxnNotFound, e.to_string())
            .suggest(["pick a commit with \"undone\": false from txn.history".to_owned()]),
        CommandError::Journal(j) => ApiError::new(ApiErrorKind::SceneIo, j.to_string()),
        other => crate::engine::internal(other),
    }
}

/// Maps a scene error.
pub(crate) fn scene(e: SceneError, hints: &Hints<'_>) -> ApiError {
    let message = e.to_string();
    match e {
        SceneError::Parse { line, col, .. } | SceneError::DuplicateEntity { line, col, .. } => {
            ApiError::new(ApiErrorKind::SceneParse, message)
                .with("line", json!(line))
                .with("col", json!(col))
        }
        SceneError::UnknownComponent {
            line,
            col,
            did_you_mean,
            ..
        } => ApiError::new(ApiErrorKind::SceneParse, message)
            .suggest(did_you_mean.map(|n| format!("did you mean \"{n}\"?")))
            .with("line", json!(line))
            .with("col", json!(col))
            .see("concordia://schema/index"),
        SceneError::InvalidValue {
            component,
            pointer,
            line,
            col,
            ..
        } => ApiError::new(ApiErrorKind::SceneParse, message)
            .at(pointer)
            .with("line", json!(line))
            .with("col", json!(col))
            .see(format!("concordia://schema/component/{component}")),
        SceneError::UnsupportedFormat { .. } => ApiError::new(ApiErrorKind::SceneParse, message),
        SceneError::NonFinite { pointer, .. } => {
            ApiError::new(ApiErrorKind::ValueInvalid, message).at(pointer)
        }
        SceneError::World(e) => ecs(e, hints),
        other => crate::engine::internal(other),
    }
}

/// An I/O failure on `path`.
pub(crate) fn io(path: &str, e: &std::io::Error) -> ApiError {
    ApiError::new(ApiErrorKind::SceneIo, format!("{path}: {e}"))
}

fn pointer_of(e: &ReflectError) -> String {
    match e {
        ReflectError::Invalid { pointer, .. }
        | ReflectError::MissingField { pointer, .. }
        | ReflectError::UnknownField { pointer, .. }
        | ReflectError::OutOfRange { pointer, .. } => pointer.clone(),
        _ => String::new(),
    }
}

/// The sub-schema at a value pointer (best effort; falls back to the nearest ancestor).
pub(crate) fn schema_at(schema: &Value, pointer: &str) -> Value {
    let mut current = schema;
    for token in pointer.split('/').skip(1) {
        let token = token.replace("~1", "/").replace("~0", "~");
        let next = current["properties"].get(&token).or_else(|| {
            let index: usize = token.parse().ok()?;
            current["prefixItems"]
                .get(index)
                .or_else(|| current.get("items").filter(|i| i.is_object()))
        });
        match next {
            Some(next) => current = next,
            None => break,
        }
    }
    current.clone()
}
