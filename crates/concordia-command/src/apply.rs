//! Applying ops to a world, each atomically, returning its inverse.

use std::collections::BTreeMap;

use concordia_core::id::EntityId;
use concordia_reflect::{TypeDesc, TypeRegistry, migrate};
use concordia_terra::{
    DynamicQuery, EcsError, World, component_from_json, register_dynamic_component,
};
use serde_json::Value;

use crate::op::{AppliedOp, EditOp};

/// Applies `op`; on error the world is unchanged.
pub(crate) fn apply(
    world: &mut World,
    reg: &mut TypeRegistry,
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
        EditOp::MigrateType { component, to } => migrate_type(world, reg, component, to),
    }
}

/// Reverts applied ops, newest first.
pub(crate) fn revert(
    world: &mut World,
    reg: &mut TypeRegistry,
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
    reg: &mut TypeRegistry,
    ops: &[AppliedOp],
) -> Result<(), EcsError> {
    ops.iter()
        .try_for_each(|applied| apply(world, reg, &applied.op).map(|_| ()))
}

fn spawn(
    world: &mut World,
    reg: &mut TypeRegistry,
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
    reg: &mut TypeRegistry,
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
    reg: &mut TypeRegistry,
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

/// Moves component type `component` to the version `to` describes; the inverse restores the
/// previous version (exactly: the registry keeps versions) and every previous value exactly.
fn migrate_type(
    world: &mut World,
    reg: &mut TypeRegistry,
    component: &str,
    to: &Value,
) -> Result<Vec<EditOp>, EcsError> {
    let fail = |message: String| EcsError::Dynamic {
        component: component.to_owned(),
        message,
    };
    let desc: TypeDesc = serde_json::from_value(to.clone())
        .map_err(|e| fail(format!("`to` is not a type description: {e}")))?;
    if desc.name != component {
        return Err(fail(format!(
            "the description is for {}, not {component}",
            desc.name
        )));
    }
    let old = current_description(reg, component)?;
    let ids = DynamicQuery {
        with: vec![component.to_owned()],
        without: Vec::new(),
    }
    .run(world, reg)?;
    let before = ids
        .iter()
        .map(|&id| Ok((id, world.get_json(id, component)?)))
        .collect::<Result<Vec<_>, EcsError>>()?;
    let info = register_dynamic_component(reg, &desc).map_err(|e| fail(e.to_string()))?;
    let migrated = world.migrate_dynamic(info, |v| migrate(v, info).map(|(new, _)| new));
    if let Err(e) = migrated {
        register_dynamic_component(reg, &old).map_err(|e| fail(e.to_string()))?;
        return Err(e);
    }
    let back = serde_json::to_value(&old).map_err(|e| fail(e.to_string()))?;
    Ok(restore_ops(component, back, before))
}

/// Undoes a migration without converting back (which can't invent removed fields): remove the
/// component, return the type to its old version with nothing stored, then set every old value.
fn restore_ops(component: &str, old: Value, before: Vec<(EntityId, Value)>) -> Vec<EditOp> {
    let removes: Vec<EditOp> = before
        .iter()
        .map(|(id, _)| EditOp::RemoveComponent {
            id: *id,
            component: component.to_owned(),
        })
        .collect();
    let back = EditOp::MigrateType {
        component: component.to_owned(),
        to: old,
    };
    let sets = before.into_iter().map(|(id, value)| EditOp::SetComponent {
        id,
        component: component.to_owned(),
        value,
    });
    removes
        .into_iter()
        .chain(std::iter::once(back))
        .chain(sets)
        .collect()
}

/// The description of script component `component`'s current version.
fn current_description(reg: &TypeRegistry, component: &str) -> Result<TypeDesc, EcsError> {
    let registration =
        reg.get_by_name(component)
            .ok_or_else(|| EcsError::ComponentNotRegistered {
                name: component.to_owned(),
            })?;
    if registration
        .data::<concordia_terra::ReflectComponent>()
        .is_none()
    {
        return Err(EcsError::NotAComponent {
            name: component.to_owned(),
        });
    }
    registration
        .desc()
        .cloned()
        .ok_or_else(|| EcsError::Dynamic {
            component: component.to_owned(),
            message: format!("{component} is a Rust type; only script types can be migrated"),
        })
}
