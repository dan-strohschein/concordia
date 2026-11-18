//! The M0 methods and the helpers they share. The only path to the edit world is [`edit`].

mod component;
mod entity;
mod log;
mod query;
mod scene;
mod schema;
mod session;
mod sim;
mod txn;

use std::sync::OnceLock;

use concordia_command::{Author, EditOp};
use concordia_core::id::{EntityId, TxnId};
use concordia_ecs::World;
use concordia_reflect::{Primitive, Reflect, Shape, TypeInfo};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::engine::{Engine, OpenScene};
use crate::error::{ApiError, ApiErrorKind};
use crate::map::{self, Hints};
use crate::method::{CallCtx, MethodInfo};

/// Every M0 method.
pub(crate) fn all() -> Vec<MethodInfo> {
    let mut methods = Vec::new();
    methods.extend(session::methods());
    methods.extend(scene::methods());
    methods.extend(entity::methods());
    methods.extend(component::methods());
    methods.extend(query::methods());
    methods.extend(schema::methods());
    methods.extend(txn::methods());
    methods.extend(sim::methods());
    methods.extend(log::methods());
    methods
}

/// An entity id given by a client (validated against the world when used).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityRef(pub String);

impl Reflect for EntityRef {
    fn type_info() -> &'static TypeInfo {
        static INFO: OnceLock<TypeInfo> = OnceLock::new();
        INFO.get_or_init(|| {
            let docs = "Entity id (UUID string). If you pass a name instead, the error suggests the matching id.";
            TypeInfo::named("EntityRef", "concordia_api::EntityRef", docs, Shape::Primitive(Primitive::Uuid))
        })
    }
}

/// Which world to read.
#[allow(non_camel_case_types)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, concordia_reflect::Reflect,
)]
pub enum WorldSel {
    /// The scene being edited (default).
    #[default]
    edit,
    /// The running simulation (exists after sim.play or sim.step).
    play,
}

/// No params or no result.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, concordia_reflect::Reflect)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

/// Resolves a client entity reference in `world`.
pub(crate) fn resolve(world: &World, given: &EntityRef) -> Result<EntityId, ApiError> {
    match given.0.parse::<EntityId>() {
        Ok(id) if world.contains(id) => Ok(id),
        _ => Err(map::entity_not_found(&given.0, Some(world))),
    }
}

/// The world a read targets.
pub(crate) fn world_of(engine: &Engine, which: Option<WorldSel>) -> Result<&World, ApiError> {
    match which.unwrap_or_default() {
        WorldSel::edit => Ok(&engine.scene()?.world),
        WorldSel::play => engine
            .play
            .as_ref()
            .map(|p| &p.world)
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NotPlaying)),
    }
}

/// Fails if a component name is not a registered component.
pub(crate) fn require_component(engine: &Engine, name: &str) -> Result<(), ApiError> {
    match map::component_names(&engine.registry).contains(&name) {
        true => Ok(()),
        false => Err(map::component_not_registered(name, &engine.registry)),
    }
}

/// `TxnConflict` naming the holder, if another session holds the open transaction.
pub(crate) fn conflict(scene: &OpenScene, ctx: &CallCtx) -> Option<ApiError> {
    let hold = scene.hold?;
    let open = scene.log.open_txn()?;
    let holder = json!({
        "session": hold.session.to_string(),
        "actor": open.author.actor.to_string(),
        "label": open.label,
        "open_for_ms": u64::try_from(ctx.now.saturating_duration_since(hold.since).as_millis()).unwrap_or(u64::MAX),
    });
    let mine = hold.session == ctx.session;
    let message = match mine {
        true => format!(
            "your transaction {} ({:?}) is still open",
            open.id, open.label
        ),
        false => format!(
            "transaction {} ({:?}) by {} is open in another session",
            open.id, open.label, open.author.actor
        ),
    };
    Some(ApiError::new(ApiErrorKind::TxnConflict, message).with("holder", holder))
}

/// Applies one edit: inside the caller's open transaction, or as its own transaction.
pub(crate) fn edit(
    engine: &mut Engine,
    ctx: &CallCtx,
    label: &str,
    op: EditOp,
    component: Option<&str>,
) -> Result<(), ApiError> {
    let author = Author {
        actor: ctx.actor()?.clone(),
        session: Some(ctx.session),
    };
    let Engine {
        scene,
        registry,
        ids,
        ..
    } = engine;
    let scene = scene
        .as_mut()
        .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NoSceneOpen))?;
    let mine = match scene.hold {
        Some(hold) if hold.session == ctx.session => true,
        Some(_) => {
            return Err(conflict(scene, ctx)
                .unwrap_or_else(|| ApiError::from_kind(ApiErrorKind::TxnConflict)));
        }
        None => false,
    };
    let result = match mine {
        true => scene.log.apply(&mut scene.world, registry, op),
        false => scene
            .log
            .apply_one(&mut scene.world, registry, author, label, op, ids)
            .map(|_| ()),
    };
    if let Err(e) = result {
        if mine {
            scene.hold = None;
        }
        let err = map::command(
            e,
            component,
            &Hints {
                registry,
                world: Some(&scene.world),
            },
        );
        let note =
            mine.then(|| "your open transaction was rolled back; begin a new one".to_owned());
        return Err(err.suggest(note));
    }
    scene.dirty = true;
    Ok(())
}

/// Parses a txn id given by a client.
pub(crate) fn txn_id(text: &str) -> Result<TxnId, ApiError> {
    text.parse()
        .map_err(|_| ApiError::from_kind(ApiErrorKind::TxnNotFound).at("/txn"))
}
