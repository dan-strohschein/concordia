//! `DestructionModule`: the `[destruction]` engine module (play only).

use std::collections::BTreeSet;
use std::sync::Arc;

use concordia_api::{ApiError, ApiErrorKind, Engine, EngineModule, MethodInfo, PlaySetup};
use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_terra::{Phase, Schedule, World, register_component};

use crate::{Breakable, DestructionSettings, Fragment, Loaded, MemorySource, destruction_system};

/// Breaks the play world's breakables; the edit world is never touched (ADR-0032).
#[derive(Debug, Clone, Copy, Default)]
pub struct DestructionModule {
    settings: DestructionSettings,
}

impl DestructionModule {
    /// The module with `settings`.
    pub fn new(settings: DestructionSettings) -> Self {
        Self { settings }
    }
}

impl EngineModule for DestructionModule {
    fn name(&self) -> &'static str {
        "destruction"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        register_component::<Breakable>(registry)?;
        register_component::<crate::Field>(registry)?;
        register_component::<crate::FieldAge>(registry)?;
        register_component::<Fragment>(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![
            MethodInfo::of::<crate::DestructionCrumble>(),
            MethodInfo::of::<crate::DestructionApplyStrain>(),
        ]
    }

    /// Loads every recipe the play world's breakables name, then schedules `destruction.break`.
    fn start_play_in(&self, engine: &mut Engine, world: &mut World) -> Result<PlaySetup, ApiError> {
        let mut recipes = BTreeSet::new();
        if let Ok(mut q) = world.query::<&Breakable>() {
            q.for_each(world, |_, b| {
                recipes.insert(b.recipe.clone());
            });
        }
        let mut source = MemorySource::default();
        for path in recipes {
            match load(engine, &path) {
                Ok(loaded) => source = source.with(&path, loaded),
                // A broken recipe leaves its breakable whole; the system warns once.
                Err(e) => {
                    tracing::warn!(target: "concordia::destruction", path, error = %e.message, "breakable not loaded")
                }
            }
        }
        let source: Arc<dyn crate::BreakableSource> = Arc::new(source);
        let mut schedule = Schedule::new();
        schedule.add_in(
            Phase::PostPhysics,
            destruction_system(source.clone(), self.settings),
        );
        Ok(PlaySetup {
            schedule,
            state: Some(Box::new(crate::methods::PlayLibrary(source))),
        })
    }
}

fn load(engine: &mut Engine, path: &str) -> Result<Loaded, ApiError> {
    let (recipe, stored) = concordia_fracture::load(engine, path)?;
    let structure = concordia_fracture::structure(&stored.fracture, &recipe.structure)
        .map_err(|e| ApiError::new(ApiErrorKind::InvalidParams, format!("{path}: {e}")))?;
    Ok(Loaded {
        fracture: stored.fracture,
        structure,
    })
}
