//! `prefab.instantiate`: spawns an imported prefab's entities as one undo step (M1 phase 1.11).

use std::collections::BTreeMap;

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method, WorldSel};
use concordia_command::{Actor, EditOp};
use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use concordia_scene::{PrefabDoc, instantiate_prefab};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::db::prefab_doc;
use crate::error::AssetError;
use crate::module::{api_error, db};
use crate::reference::AssetRef;

/// Spawn a fresh copy of an imported model's (prefab's) entity hierarchy as one undo step; the root can be parented and placed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct PrefabInstantiate;

/// `prefab.instantiate` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PrefabInstantiateParams {
    /// The prefab: an imported model, e.g. "asset:<id>" from asset.import or project.files.
    pub asset: AssetRef,
    /// Parent entity for the new root (default: none, a top-level entity).
    #[serde(default)]
    pub parent: Option<EntityId>,
    /// The root's local translation (default: the prefab's own, usually the origin).
    #[serde(default)]
    pub translation: Option<[f32; 3]>,
    /// Who is editing.
    pub actor: Option<Actor>,
}

/// `prefab.instantiate` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct PrefabInstantiateResult {
    /// The new root entity.
    pub root: EntityId,
    /// Every new entity, root first.
    pub entities: Vec<EntityId>,
}

impl Method for PrefabInstantiate {
    const NAME: &'static str = "prefab.instantiate";
    const MUTATES: bool = true;
    type Params = PrefabInstantiateParams;
    type Result = PrefabInstantiateResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: PrefabInstantiateParams,
    ) -> Result<PrefabInstantiateResult, ApiError> {
        check_parent(engine, p.parent)?;
        let prefab = load(engine, &p.asset)?;
        let (root, mut ops) =
            instantiate_prefab(&prefab, engine.ids_mut()).map_err(|e| invalid(e.to_string()))?;
        if let Some(EditOp::Spawn { components, .. }) = ops.first_mut() {
            place(components, p.parent, p.translation);
        }
        let entities = ops
            .iter()
            .filter_map(|op| match op {
                EditOp::Spawn { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        engine.apply_edits(ctx, &format!("Instantiate {}", prefab.name), ops)?;
        Ok(PrefabInstantiateResult { root, entities })
    }
}

/// A parent must exist in the edit world.
fn check_parent(engine: &Engine, parent: Option<EntityId>) -> Result<(), ApiError> {
    match parent {
        Some(id) if !engine.world(WorldSel::edit)?.contains(id) => Err(ApiError::new(
            ApiErrorKind::EntityNotFound,
            format!("parent entity {id} does not exist"),
        )
        .at("/parent")),
        _ => Ok(()),
    }
}

/// Reads the prefab artifact `asset` names.
fn load(engine: &mut Engine, asset: &AssetRef) -> Result<PrefabDoc, ApiError> {
    let (kind, bytes) = db(engine)?
        .load_artifact(asset)
        .map_err(|e| api_error(e).at("/asset"))?;
    prefab_doc(asset, kind, bytes, engine.registry()).map_err(|e| match e {
        AssetError::Decode(m) => invalid(m).at("/asset"),
        e => api_error(e).at("/asset"),
    })
}

/// Sets the root's parent and translation.
fn place(components: &mut BTreeMap<String, Value>, parent: Option<EntityId>, at: Option<[f32; 3]>) {
    if let Some(parent) = parent {
        components.insert("Parent".to_owned(), json!(parent));
    }
    if let (Some(at), Some(Value::Object(transform))) = (at, components.get_mut("Transform")) {
        transform.insert("translation".to_owned(), json!(at));
    }
}

fn invalid(message: String) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidParams, message)
}
