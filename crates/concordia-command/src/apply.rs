//! Applying ops to a world, each atomically, returning its inverse.

use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use concordia_ecs::{EcsError, World, component_from_json};
use concordia_reflect::TypeRegistry;
use serde_json::Value;

use crate::op::{AppliedOp, EditOp};

/// Applies `op`; on error the world is unchanged.
pub(crate) fn apply(
    world: &mut World,
    reg: &TypeRegistry,
    op: &EditOp,
) -> Result<Vec<EditOp>, EcsError> {
    match op {
        EditOp::Spawn { id, components } => spawn(world, reg, *id, components),
        EditOp::Despawn { id } => despawn(world, *id),
        EditOp::SetComponent {
            id,
            component,
            value,
        } => set(world, reg, *id, component, value.clone()),
        EditOp::RemoveComponent { id, component } => remove(world, reg, *id, component),
    }
}

/// Reverts applied ops, newest first.
pub(crate) fn revert(
    world: &mut World,
    reg: &TypeRegistry,
    ops: &[AppliedOp],
) -> Result<(), EcsError> {
    for applied in ops.iter().rev() {
        for inverse in &applied.inverse {
            apply(world, reg, inverse)?;
        }
    }
    Ok(())
}

/// Re-applies ops in order.
pub(crate) fn reapply(
    world: &mut World,
    reg: &TypeRegistry,
    ops: &[AppliedOp],
) -> Result<(), EcsError> {
    ops.iter()
        .try_for_each(|applied| apply(world, reg, &applied.op).map(|_| ()))
}

fn spawn(
    world: &mut World,
    reg: &TypeRegistry,
    id: EntityId,
    components: &BTreeMap<String, Value>,
) -> Result<Vec<EditOp>, EcsError> {
    let values = components
        .iter()
        .map(|(name, json)| component_from_json(reg, name, json.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    world.spawn_with_id(id)?;
    for value in values {
        if let Err(e) = world.insert_dyn(id, value, reg) {
            world.despawn(id)?;
            return Err(e);
        }
    }
    Ok(vec![EditOp::Despawn { id }])
}

fn despawn(world: &mut World, id: EntityId) -> Result<Vec<EditOp>, EcsError> {
    let names = world
        .component_names(id)
        .ok_or(EcsError::NoSuchEntity(id))?;
    let components = names
        .into_iter()
        .map(|name| Ok((name.to_owned(), world.get_json(id, name)?)))
        .collect::<Result<BTreeMap<_, _>, EcsError>>()?;
    world.despawn(id)?;
    Ok(vec![EditOp::Spawn { id, components }])
}

fn set(
    world: &mut World,
    reg: &TypeRegistry,
    id: EntityId,
    component: &str,
    value: Value,
) -> Result<Vec<EditOp>, EcsError> {
    let inverse = match world.set_json(id, component, value, reg)? {
        Some(old) => EditOp::SetComponent {
            id,
            component: component.to_owned(),
            value: old,
        },
        None => EditOp::RemoveComponent {
            id,
            component: component.to_owned(),
        },
    };
    Ok(vec![inverse])
}

fn remove(
    world: &mut World,
    reg: &TypeRegistry,
    id: EntityId,
    component: &str,
) -> Result<Vec<EditOp>, EcsError> {
    let old = world
        .remove_dyn(id, component)?
        .ok_or_else(|| EcsError::ComponentMissing {
            entity: id,
            component: component.to_owned(),
        })?;
    match old.to_json() {
        Ok(value) => Ok(vec![EditOp::SetComponent {
            id,
            component: component.to_owned(),
            value,
        }]),
        Err(e) => {
            world.insert_dyn(id, old, reg)?;
            Err(e.into())
        }
    }
}
