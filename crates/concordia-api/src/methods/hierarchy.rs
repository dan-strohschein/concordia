//! Templates and hierarchy editing (M1 phase 1.13): `entity.templates`, `entity.create`,
//! `entity.duplicate` and `entity.reparent`. Each edit is one undo step, or joins the caller's
//! open transaction.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use concordia_command::{Actor, EditOp};
use concordia_core::id::{EntityId, IdSource};
use concordia_core::math::{Mat4, Vec3};
use concordia_reflect::Reflect;
use concordia_scene::builtin::{Name, Parent, Transform, world_transform};
use concordia_terra::World;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Empty, EntityRef, resolve};
use crate::engine::{Engine, internal};
use crate::error::{ApiError, ApiErrorKind};
use crate::method::{CallCtx, Method, MethodInfo};

/// List the ready-made entities (templates) that entity.create can spawn, with what each is for and its components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EntityTemplates;

/// `entity.templates` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EntityTemplatesResult {
    /// Templates sorted by name: module built-ins and the project's templates/**/*.template.ron.
    pub templates: Vec<TemplateSummary>,
}

/// One template, as listed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TemplateSummary {
    /// Name to pass to entity.create or terra.spawn_batch.
    pub name: String,
    /// What it is for.
    pub description: String,
    /// The traits it composes, by name.
    pub traits: Vec<String>,
    /// The components it starts with (from its traits and component values).
    pub components: Vec<String>,
    /// Why it can't be spawned, if it can't (e.g. two traits add one component).
    pub problem: Option<String>,
}

impl Method for EntityTemplates {
    const NAME: &'static str = "entity.templates";
    const MUTATES: bool = false;
    type Params = Empty;
    type Result = EntityTemplatesResult;

    fn call(engine: &mut Engine, _: &CallCtx, _: Empty) -> Result<EntityTemplatesResult, ApiError> {
        let catalog = crate::templates::catalog(engine)?;
        let templates = catalog
            .iter()
            .map(|t| {
                let compiled = t.compile(engine.registry());
                TemplateSummary {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    traits: t.traits.keys().cloned().collect(),
                    components: match &compiled {
                        Ok(c) => c.component_names().into_iter().map(str::to_owned).collect(),
                        Err(_) => t.components.keys().cloned().collect(),
                    },
                    problem: compiled.err().map(|e| e.to_string()),
                }
            })
            .collect();
        Ok(EntityTemplatesResult { templates })
    }
}

/// Spawn a ready-made entity (see entity.templates), optionally named, parented and placed; sibling names are kept unique ("Cube 2").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EntityCreate;

/// `entity.create` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct EntityCreateParams {
    /// Template name from entity.templates, e.g. "Point Light".
    pub template: String,
    /// Parent entity (default: none, a top-level entity).
    #[serde(default)]
    pub parent: Option<EntityRef>,
    /// Entity name (default: the template's); made unique among its siblings.
    #[serde(default)]
    pub name: Option<String>,
    /// Local translation (default: the template's, usually the origin).
    #[serde(default)]
    pub translation: Option<[f32; 3]>,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// The new entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct CreatedEntity {
    /// Its id.
    pub id: EntityId,
}

impl Method for EntityCreate {
    const NAME: &'static str = "entity.create";
    const MUTATES: bool = true;
    type Params = EntityCreateParams;
    type Result = CreatedEntity;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: EntityCreateParams,
    ) -> Result<CreatedEntity, ApiError> {
        let template = crate::templates::compiled(engine, &p.template)?;
        let world = &engine.scene()?.world;
        let parent = p
            .parent
            .as_ref()
            .map(|r| resolve_parent(world, r))
            .transpose()?;
        let mut components = template.components_json();
        let own_name = components
            .get("Name")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let base = p.name.or(own_name).unwrap_or(template.name);
        components.insert("Name".into(), json!(unique_name(world, parent, &base)));
        if let Some(parent) = parent {
            components.insert("Parent".into(), json!(parent));
        }
        if let (Some(at), Some(Value::Object(t))) = (p.translation, components.get_mut("Transform"))
        {
            t.insert("translation".into(), json!(at));
        }
        let id = engine.ids.next_id().map_err(internal)?;
        engine.apply_edits(ctx, Self::NAME, vec![EditOp::Spawn { id, components }])?;
        Ok(CreatedEntity { id })
    }
}

/// Copy entities with all their descendants (new ids; references inside the copies point at the copies) as one undo step. Copies sit beside their originals with unique names ("Crate 2").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EntityDuplicate;

/// `entity.duplicate` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct EntityDuplicateParams {
    /// Entities to copy; one that is a descendant of another listed entity is copied with it.
    pub ids: Vec<EntityRef>,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

/// `entity.duplicate` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct DuplicatedEntities {
    /// The copies' root ids, in the order given.
    pub ids: Vec<EntityId>,
}

impl Method for EntityDuplicate {
    const NAME: &'static str = "entity.duplicate";
    const MUTATES: bool = true;
    type Params = EntityDuplicateParams;
    type Result = DuplicatedEntities;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: EntityDuplicateParams,
    ) -> Result<DuplicatedEntities, ApiError> {
        if p.ids.is_empty() {
            return Err(ApiError::new(ApiErrorKind::InvalidParams, "ids is empty").at("/ids"));
        }
        let world = &engine.scene()?.world;
        let given = p
            .ids
            .iter()
            .map(|r| resolve(world, r))
            .collect::<Result<Vec<_>, _>>()?;
        let roots = top_most(world, &given);
        let children = children_of(world);
        let mut plan = Vec::new();
        for root in &roots {
            plan.extend(subtree(&children, *root));
        }
        let mut mapping = HashMap::new();
        for old in &plan {
            mapping.insert(old.to_string(), engine.ids.next_id().map_err(internal)?);
        }
        let world = &engine.scene()?.world;
        let ops = plan
            .iter()
            .map(|old| copy_op(world, *old, &mapping, roots.contains(old)))
            .collect::<Result<Vec<_>, _>>()?;
        engine.apply_edits(ctx, Self::NAME, ops)?;
        let ids = roots.iter().map(|r| mapping[&r.to_string()]).collect();
        Ok(DuplicatedEntities { ids })
    }
}

/// Move an entity under a new parent (null: top level). By default its world position, rotation and scale stay the same; a move that would make an entity its own ancestor is rejected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EntityReparent;

/// `entity.reparent` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct EntityReparentParams {
    /// The entity to move.
    pub id: EntityRef,
    /// The new parent, or null for top level.
    pub parent: Option<EntityRef>,
    /// Keep the world transform (default true); false keeps the local transform instead.
    #[serde(default)]
    pub keep_world: Option<bool>,
    /// Actor (defaults to the session's).
    pub actor: Option<Actor>,
}

impl Method for EntityReparent {
    const NAME: &'static str = "entity.reparent";
    const MUTATES: bool = true;
    type Params = EntityReparentParams;
    type Result = Empty;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: EntityReparentParams,
    ) -> Result<Empty, ApiError> {
        let world = &engine.scene()?.world;
        let id = resolve(world, &p.id)?;
        let parent = p
            .parent
            .as_ref()
            .map(|r| resolve_parent(world, r))
            .transpose()?;
        if let Some(parent) = parent {
            reject_cycle(world, id, parent)?;
        }
        let current = world.get::<Parent>(id).map(|p| p.0);
        if current == parent {
            return Ok(Empty {});
        }
        let mut ops = vec![match parent {
            Some(parent) => EditOp::SetComponent {
                id,
                component: "Parent".into(),
                value: json!(parent),
            },
            None => EditOp::RemoveComponent {
                id,
                component: "Parent".into(),
            },
        }];
        if p.keep_world.unwrap_or(true) {
            ops.extend(keep_world_op(world, id, parent)?);
        }
        engine.apply_edits(ctx, Self::NAME, ops)?;
        Ok(Empty {})
    }
}

fn resolve_parent(world: &World, given: &EntityRef) -> Result<EntityId, ApiError> {
    resolve(world, given).map_err(|_| {
        ApiError::new(
            ApiErrorKind::EntityNotFound,
            format!("parent entity {} does not exist", given.0),
        )
        .at("/parent")
    })
}

/// Children by parent, each list sorted by id.
pub(crate) fn children_of(world: &World) -> BTreeMap<EntityId, Vec<EntityId>> {
    let mut map: BTreeMap<EntityId, Vec<EntityId>> = BTreeMap::new();
    for id in world.entity_ids_sorted() {
        if let Some(Parent(parent)) = world.get::<Parent>(id) {
            map.entry(*parent).or_default().push(id);
        }
    }
    map
}

/// `root` and all its descendants, parents before children (cycle-safe).
pub(crate) fn subtree(
    children: &BTreeMap<EntityId, Vec<EntityId>>,
    root: EntityId,
) -> Vec<EntityId> {
    let mut out = vec![root];
    let mut seen = BTreeSet::from([root]);
    let mut i = 0;
    while let Some(&id) = out.get(i) {
        for &child in children.get(&id).into_iter().flatten() {
            if seen.insert(child) {
                out.push(child);
            }
        }
        i += 1;
    }
    out
}

/// The given entities without those that have another given entity as an ancestor (order kept, no repeats).
fn top_most(world: &World, given: &[EntityId]) -> Vec<EntityId> {
    let set: BTreeSet<EntityId> = given.iter().copied().collect();
    let mut seen = BTreeSet::new();
    given
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .filter(|id| !ancestors(world, *id).iter().any(|a| set.contains(a)))
        .collect()
}

/// Parent, grandparent, ... (stops at a cycle).
fn ancestors(world: &World, id: EntityId) -> Vec<EntityId> {
    let mut out = Vec::new();
    let mut at = id;
    while let Some(Parent(parent)) = world.get::<Parent>(at) {
        if *parent == id || out.contains(parent) {
            break;
        }
        out.push(*parent);
        at = *parent;
    }
    out
}

fn reject_cycle(world: &World, id: EntityId, parent: EntityId) -> Result<(), ApiError> {
    match parent == id || ancestors(world, parent).contains(&id) {
        true => Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("making {parent} the parent of {id} would create a cycle"),
        )
        .at("/parent")),
        false => Ok(()),
    }
}

/// `base`, or "base N" with the smallest free N ≥ 2, among the children of `parent` (or top-level entities).
fn unique_name(world: &World, parent: Option<EntityId>, base: &str) -> String {
    let taken: BTreeSet<String> = world
        .entity_ids_sorted()
        .into_iter()
        .filter(|id| world.get::<Parent>(*id).map(|p| p.0) == parent)
        .filter_map(|id| world.get::<Name>(id).map(|n| n.0.clone()))
        .collect();
    if !taken.contains(base) {
        return base.to_owned();
    }
    let stem = strip_number(base);
    (2u32..)
        .map(|n| format!("{stem} {n}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| base.to_owned())
}

/// "Crate 3" → "Crate"; other names unchanged.
fn strip_number(name: &str) -> &str {
    match name.rsplit_once(' ') {
        Some((stem, n)) if !stem.is_empty() && n.parse::<u32>().is_ok() => stem,
        _ => name,
    }
}

/// A spawn op copying `old` under its new id, references remapped; a copied root gets a unique name.
fn copy_op(
    world: &World,
    old: EntityId,
    mapping: &HashMap<String, EntityId>,
    root: bool,
) -> Result<EditOp, ApiError> {
    let mut components = BTreeMap::new();
    for name in world.component_names(old).unwrap_or_default() {
        let value = world.get_json(old, name).map_err(internal)?;
        components.insert(name.to_owned(), concordia_scene::remap(value, mapping));
    }
    if root && let Some(Name(name)) = world.get::<Name>(old) {
        let parent = world.get::<Parent>(old).map(|p| p.0);
        components.insert("Name".into(), json!(unique_name(world, parent, name)));
    }
    Ok(EditOp::Spawn {
        id: mapping[&old.to_string()],
        components,
    })
}

/// The Transform that keeps `id`'s world pose under `parent`.
fn keep_world_op(
    world: &World,
    id: EntityId,
    parent: Option<EntityId>,
) -> Result<Option<EditOp>, ApiError> {
    let Some(current) = world_transform(world, id) else {
        return Ok(None);
    };
    let parent_world = parent
        .and_then(|p| world_transform(world, p))
        .unwrap_or(Mat4::IDENTITY);
    let (scale, rotation, translation) =
        (parent_world.inverse() * current).to_scale_rotation_translation();
    if !(scale.is_finite() && rotation.is_finite() && translation.is_finite())
        || scale.min_element() <= 0.0
    {
        return Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            "the new parent's transform cannot be inverted (zero scale?); pass keep_world: false",
        )
        .at("/parent"));
    }
    let local = Transform {
        translation,
        rotation: rotation.normalize(),
        scale: snap(scale),
    };
    Ok(Some(EditOp::SetComponent {
        id,
        component: "Transform".into(),
        value: serde_json::to_value(local).map_err(internal)?,
    }))
}

/// Rounds away float noise from matrix decomposition (1.0000001 → 1.0).
fn snap(v: Vec3) -> Vec3 {
    let round = |x: f32| {
        let r = (x * 1e5).round() / 1e5;
        if (r - x).abs() < 1e-5 { r } else { x }
    };
    Vec3::new(round(v.x), round(v.y), round(v.z))
}

pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<EntityTemplates>(),
        MethodInfo::of::<EntityCreate>(),
        MethodInfo::of::<EntityDuplicate>(),
        MethodInfo::of::<EntityReparent>(),
    ]
}
