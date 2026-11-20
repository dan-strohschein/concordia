//! Registering a module's types: helper types, components and signals as dynamic reflect types,
//! then traits.

use concordia_reflect::{TypeDesc, TypeRegistry};
use concordia_terra::register_dynamic_component;
use concordia_terra_sim::{DynamicTrait, register_dynamic_trait};

use crate::{ModuleDesc, ScriptError};

/// Validates `desc`, then registers (or, on reload, replaces) its helper types, components,
/// signals and traits in `registry`.
pub fn register_module(registry: &mut TypeRegistry, desc: &ModuleDesc) -> Result<(), ScriptError> {
    desc.validate()?;
    let fail = |name: &str, reason: String| ScriptError::Type {
        module: desc.name.clone(),
        name: name.to_owned(),
        reason,
    };
    for ty in &desc.types {
        registry
            .register_dynamic(ty)
            .map_err(|e| fail(&ty.name, e.to_string()))?;
    }
    for ty in desc.components.iter().chain(&desc.signals) {
        register_dynamic_component(registry, ty).map_err(|e| fail(&ty.name, e.to_string()))?;
    }
    for t in &desc.traits {
        let spec = DynamicTrait {
            name: t.name.clone(),
            docs: t.docs.clone(),
            components: t.components.clone(),
            requires: t.requires.clone(),
        };
        register_dynamic_trait(registry, &spec).map_err(|e| fail(&t.name, e.to_string()))?;
    }
    Ok(())
}

/// Every type a description declares, in registration order.
pub(crate) fn declared_types(desc: &ModuleDesc) -> impl Iterator<Item = &TypeDesc> {
    desc.types
        .iter()
        .chain(&desc.components)
        .chain(&desc.signals)
}
