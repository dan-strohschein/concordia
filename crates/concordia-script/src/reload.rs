//! Reloading a module's types (ADR-0061): only changed types are re-registered, and every
//! stored value, tag value and pending signal of them moves to the new version through
//! reflect's `migrate`, all at once. If any value can't convert, the old versions come back
//! and the world is left as it was.

use std::collections::BTreeMap;

use concordia_reflect::{DynamicValue, TypeDesc, TypeInfo, TypeRegistry, migrate};
use concordia_terra::{EcsError, World, register_dynamic_component};
use concordia_terra_sim::{DynamicTrait, register_dynamic_trait};

use crate::host::TypeMigration;
use crate::{ModuleDesc, ScriptError};

/// The component and signal types to re-register: those that changed or are new (all of them
/// when a helper type changed, since components refer to helpers).
fn changed<'a>(
    old: &'a ModuleDesc,
    new: &'a ModuleDesc,
) -> Vec<(Option<&'a TypeDesc>, &'a TypeDesc)> {
    let previous: BTreeMap<&str, &TypeDesc> = old
        .components
        .iter()
        .chain(&old.signals)
        .map(|t| (t.name.as_str(), t))
        .collect();
    let helpers_changed = old.types != new.types;
    new.components
        .iter()
        .chain(&new.signals)
        .map(|t| (previous.get(t.name.as_str()).copied(), t))
        .filter(|(before, now)| helpers_changed || *before != Some(*now))
        .collect()
}

/// Registers helper `types` and component/signal `descs`; returns the latter's new infos.
fn register(
    registry: &mut TypeRegistry,
    module: &str,
    types: &[TypeDesc],
    descs: &[&TypeDesc],
) -> Result<Vec<&'static TypeInfo>, ScriptError> {
    let fail = |name: &str, reason: String| ScriptError::Type {
        module: module.to_owned(),
        name: name.to_owned(),
        reason,
    };
    for ty in types {
        registry
            .register_dynamic(ty)
            .map_err(|e| fail(&ty.name, e.to_string()))?;
    }
    descs
        .iter()
        .map(|d| register_dynamic_component(registry, d).map_err(|e| fail(&d.name, e.to_string())))
        .collect()
}

/// Moves the world to `new`'s types; returns what was migrated. On failure nothing changes.
pub(crate) fn migrate_types(
    registry: &mut TypeRegistry,
    world: &mut World,
    old: &ModuleDesc,
    new: &ModuleDesc,
) -> Result<Vec<TypeMigration>, ScriptError> {
    let changed = changed(old, new);
    let helpers = if old.types == new.types {
        &[][..]
    } else {
        &new.types[..]
    };
    let descs: Vec<&TypeDesc> = changed.iter().map(|(_, now)| *now).collect();
    let infos = register(registry, &new.name, helpers, &descs)?;
    let mut fields: BTreeMap<&'static str, (Vec<String>, Vec<String>)> = BTreeMap::new();
    let converted = world.migrate_dynamic_all(&infos, |value, to| {
        let (migrated, report) = migrate(value, to)?;
        let entry = fields.entry(to.name).or_default();
        union(&mut entry.0, report.added);
        union(&mut entry.1, report.removed);
        Ok::<DynamicValue, concordia_reflect::ReflectError>(migrated)
    });
    match converted {
        Ok(counts) => {
            register_traits(registry, new)?;
            Ok(infos
                .iter()
                .zip(counts)
                .map(|(info, values)| {
                    let (added, removed) = fields.remove(info.name).unwrap_or_default();
                    TypeMigration {
                        name: info.name.to_owned(),
                        values,
                        added,
                        removed,
                    }
                })
                .collect())
        }
        Err(e) => {
            restore(registry, world, old, &changed)?;
            Err(migration_error(&new.name, e))
        }
    }
}

fn union(into: &mut Vec<String>, more: Vec<String>) {
    for field in more {
        if !into.contains(&field) {
            into.push(field);
        }
    }
}

/// Puts the old versions back: registers them again and moves the world (still holding the
/// old shapes) onto those registrations.
fn restore(
    registry: &mut TypeRegistry,
    world: &mut World,
    old: &ModuleDesc,
    changed: &[(Option<&TypeDesc>, &TypeDesc)],
) -> Result<(), ScriptError> {
    let helpers = &old.types[..];
    let previous: Vec<&TypeDesc> = changed.iter().filter_map(|(before, _)| *before).collect();
    let infos = register(registry, &old.name, helpers, &previous)?;
    world
        .migrate_dynamic_all(&infos, |value, to| migrate(value, to).map(|(v, _)| v))
        .map_err(|e| migration_error(&old.name, e))?;
    register_traits(registry, old)
}

fn migration_error(module: &str, e: EcsError) -> ScriptError {
    let (name, reason) = match e {
        EcsError::Dynamic { component, message } => (component, message),
        other => (String::new(), other.to_string()),
    };
    ScriptError::Migration {
        module: module.to_owned(),
        name,
        reason,
    }
}

/// (Re-)registers `desc`'s traits against the current component versions.
pub(crate) fn register_traits(
    registry: &mut TypeRegistry,
    desc: &ModuleDesc,
) -> Result<(), ScriptError> {
    for t in &desc.traits {
        let spec = DynamicTrait {
            name: t.name.clone(),
            docs: t.docs.clone(),
            components: t.components.clone(),
            requires: t.requires.clone(),
        };
        register_dynamic_trait(registry, &spec).map_err(|e| ScriptError::Type {
            module: desc.name.clone(),
            name: t.name.clone(),
            reason: e.to_string(),
        })?;
    }
    Ok(())
}
