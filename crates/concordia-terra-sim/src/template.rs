//! Templates: a description, traits with parameters, and component values.

use std::collections::BTreeMap;

use concordia_core::id::{EntityId, IdSource};
use concordia_core::math::Vec3;
use concordia_core::suggest::did_you_mean;
use concordia_reflect::{DynReflect, ReflectError, TypeRegistry};
use concordia_scene::builtin::Transform;
use concordia_scene::{FORMAT_VERSION, TemplateDoc};
use concordia_terra::{CommandBuffer, EcsError, Pending, ReflectComponent, World};
use serde_json::Value;

use crate::error::TemplateError;
use crate::traits::{Part, ReflectTrait, TemplateBuilder, trait_names};

/// The source name of explicit component values (they override traits).
pub(crate) const COMPONENTS: &str = "components";

/// A template by name, as JSON (built-ins are written in code; files are read from text).
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    /// Name: the file stem for project templates.
    pub name: String,
    /// What it is for.
    pub description: String,
    /// Trait parameters by trait name.
    pub traits: BTreeMap<String, Value>,
    /// Component values by component name (override what traits add).
    pub components: BTreeMap<String, Value>,
}

impl Template {
    /// A template of plain component values.
    pub fn new<'a>(
        name: &str,
        description: &str,
        components: impl IntoIterator<Item = (&'a str, Value)>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            description: description.to_owned(),
            traits: BTreeMap::new(),
            components: components
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
        }
    }

    /// The template named `name` from a read document.
    pub fn from_doc(name: &str, doc: &TemplateDoc) -> Result<Self, TemplateError> {
        Ok(Self {
            name: name.to_owned(),
            description: doc.description.clone(),
            traits: to_json(&doc.traits)?,
            components: to_json(&doc.components)?,
        })
    }

    /// This template as a document, for writing.
    pub fn to_doc(&self, registry: &TypeRegistry) -> Result<TemplateDoc, TemplateError> {
        let typed = |values: &BTreeMap<String, Value>| {
            values
                .iter()
                .map(|(name, json)| {
                    let value = registry
                        .from_json(name, json.clone())
                        .map_err(|e| invalid(name, &e))?;
                    Ok((name.clone(), value))
                })
                .collect::<Result<BTreeMap<_, _>, TemplateError>>()
        };
        Ok(TemplateDoc {
            format: FORMAT_VERSION,
            description: self.description.clone(),
            traits: typed(&self.traits)?,
            components: typed(&self.components)?,
        })
    }

    /// Applies the traits (in name order), then the component values, checking every name and
    /// value. Two traits adding one component is a conflict; component values override.
    pub fn compile(&self, registry: &TypeRegistry) -> Result<CompiledTemplate, TemplateError> {
        let mut builder = TemplateBuilder::new(registry);
        let mut requires = Vec::new();
        for (name, params) in &self.traits {
            let build = trait_of(registry, name)?;
            let value = registry
                .from_json(name, params.clone())
                .map_err(|e| invalid(name, &e))?;
            builder.source(name);
            requires.extend(build.build(value.as_ref(), &mut builder)?);
        }
        builder.source(COMPONENTS);
        for (name, json) in &self.components {
            component_of(registry, name)?;
            let value = registry
                .from_json(name, json.clone())
                .map_err(|e| invalid(name, &e))?;
            builder.put(value, false)?;
        }
        requires.sort_unstable();
        requires.dedup();
        Ok(CompiledTemplate {
            name: self.name.clone(),
            parts: builder.parts.into_iter().collect(),
            requires,
        })
    }
}

fn to_json(
    values: &BTreeMap<String, Box<dyn DynReflect>>,
) -> Result<BTreeMap<String, Value>, TemplateError> {
    values
        .iter()
        .map(|(k, v)| Ok((k.clone(), v.to_json().map_err(|e| invalid(k, &e))?)))
        .collect()
}

fn invalid(what: &str, e: &ReflectError) -> TemplateError {
    TemplateError::InvalidValue {
        what: what.to_owned(),
        reason: e.to_string(),
    }
}

fn trait_of(registry: &TypeRegistry, name: &str) -> Result<ReflectTrait, TemplateError> {
    let Some(registration) = registry.get_by_name(name) else {
        return Err(TemplateError::UnknownTrait {
            name: name.to_owned(),
            did_you_mean: did_you_mean(name, trait_names(registry))
                .first()
                .map(|s| (*s).to_owned()),
        });
    };
    registration
        .data::<ReflectTrait>()
        .cloned()
        .ok_or_else(|| TemplateError::NotATrait {
            name: name.to_owned(),
        })
}

fn component_of(registry: &TypeRegistry, name: &str) -> Result<(), TemplateError> {
    let is_component = |n: &str| {
        registry
            .get_by_name(n)
            .is_some_and(|r| r.data::<ReflectComponent>().is_some())
    };
    if is_component(name) {
        return Ok(());
    }
    let components = registry.names().filter(|n| is_component(n));
    Err(TemplateError::UnknownComponent {
        name: name.to_owned(),
        did_you_mean: did_you_mean(name, components)
            .first()
            .map(|s| (*s).to_owned()),
    })
}

/// A checked template, ready to spawn.
pub struct CompiledTemplate {
    /// The template's name.
    pub name: String,
    parts: Vec<(String, Part)>,
    requires: Vec<String>,
}

impl std::fmt::Debug for CompiledTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledTemplate")
            .field("name", &self.name)
            .field("components", &self.component_names())
            .finish()
    }
}

impl CompiledTemplate {
    /// Component names, sorted.
    pub fn component_names(&self) -> Vec<&str> {
        self.parts.iter().map(|(n, _)| n.as_str()).collect()
    }

    /// Processors or systems the traits need, sorted and unique.
    pub fn requires(&self) -> &[String] {
        &self.requires
    }

    /// Of [`CompiledTemplate::requires`], those not in `available`.
    pub fn missing_processors<'a>(
        &self,
        available: impl IntoIterator<Item = &'a str>,
    ) -> Vec<String> {
        let available: Vec<&str> = available.into_iter().collect();
        self.requires
            .iter()
            .filter(|r| !available.contains(&r.as_str()))
            .cloned()
            .collect()
    }

    /// Component values as JSON, by name (for edit-world commands).
    pub fn components_json(&self) -> BTreeMap<String, Value> {
        self.parts
            .iter()
            .filter_map(|(n, p)| Some((n.clone(), p.value.to_json().ok()?)))
            .collect()
    }

    /// Spawns one entity with id `id`. Shared components are stored shared where the world
    /// hasn't already stored the type densely (hashes are the same either way).
    pub fn spawn(&self, world: &mut World, id: EntityId) -> Result<(), TemplateError> {
        world.spawn_with_id(id)?;
        for (_, part) in &self.parts {
            if part.shared {
                match part.component.register_shared(world) {
                    Ok(()) | Err(EcsError::StorageKindInUse { .. }) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            part.component.insert(world, id, part.value.clone_dyn())?;
        }
        Ok(())
    }

    /// Records spawning one entity from this template into `commands` (applied with the
    /// buffer, at phase end for processors).
    pub fn record(&self, commands: &mut CommandBuffer) -> Pending {
        let entity = commands.spawn();
        for (_, part) in &self.parts {
            commands.insert_reflect(entity, part.component, part.value.clone_dyn());
        }
        entity
    }

    /// Spawns `count` entities with ids from `ids`; `place(i)` gives entity `i` a translation.
    pub fn spawn_batch(
        &self,
        world: &mut World,
        ids: &mut impl IdSource,
        count: u32,
        mut place: impl FnMut(usize) -> Option<Vec3>,
    ) -> Result<Vec<EntityId>, TemplateError> {
        let mut spawned = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let id = ids.next_id().map_err(EcsError::from)?;
            self.spawn(world, id)?;
            if let Some(at) = place(i) {
                let mut transform = world.get::<Transform>(id).copied().unwrap_or_default();
                transform.translation = at;
                world.insert(id, transform)?;
            }
            spawned.push(id);
        }
        Ok(spawned)
    }
}
